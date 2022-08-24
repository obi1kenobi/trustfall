use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    rc::Rc,
    sync::Arc,
};

use regex::Regex;

use crate::{
    interpreter::{
        filtering::{
            contains, equals, greater_than, greater_than_or_equal, has_prefix, has_substring,
            has_suffix, less_than, less_than_or_equal, one_of, regex_matches_optimized,
            regex_matches_slow_path,
        },
        ValueOrVec,
    },
    ir::{
        indexed::IndexedQuery, Argument, ContextField, EdgeParameters, Eid, FieldRef, FieldValue,
        FoldSpecificFieldKind, IREdge, IRFold, IRQueryComponent, IRVertex, LocalField, Operation,
        Recursive, Vid,
    },
    util::BTreeMapTryInsertExt,
};

use super::{error::QueryArgumentsError, Adapter, DataContext, InterpretedQuery};

#[allow(clippy::type_complexity)]
pub fn interpret_ir<'query, DataToken>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    indexed_query: Arc<IndexedQuery>,
    arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
) -> Result<Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'query>, QueryArgumentsError>
where
    DataToken: Clone + Debug + 'query,
{
    let query = InterpretedQuery::from_query_and_arguments(indexed_query, arguments)?;
    let ir_query = &query.indexed_query.ir_query;

    let root_edge = &ir_query.root_name;
    let root_edge_parameters = &ir_query.root_parameters;

    let mut adapter_ref = adapter.borrow_mut();
    let mut iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> = Box::new(
        adapter_ref
            .get_starting_tokens(
                root_edge.clone(),
                root_edge_parameters.clone(),
                query.clone(),
                ir_query.root_component.root,
            )
            .map(|x| DataContext::new(Some(x))),
    );
    drop(adapter_ref);

    let component = &ir_query.root_component;
    iterator = compute_component(adapter.clone(), &query, component, iterator);

    Ok(construct_outputs(adapter.as_ref(), &query, iterator))
}

fn coerce_if_needed<'query, DataToken>(
    adapter: &RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>,
    query: &InterpretedQuery,
    vertex: &IRVertex,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>
where
    DataToken: Clone + Debug + 'query,
{
    match vertex.coerced_from_type.as_ref() {
        None => iterator,
        Some(coerced_from) => perform_coercion(
            adapter,
            query,
            vertex,
            coerced_from.clone(),
            vertex.type_name.clone(),
            iterator,
        ),
    }
}

fn perform_coercion<'query, DataToken>(
    adapter: &RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>,
    query: &InterpretedQuery,
    vertex: &IRVertex,
    coerced_from: Arc<str>,
    coerce_to: Arc<str>,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>
where
    DataToken: Clone + Debug + 'query,
{
    let mut adapter_ref = adapter.borrow_mut();
    let coercion_iter = adapter_ref.can_coerce_to_type(
        iterator,
        coerced_from,
        coerce_to,
        query.clone(),
        vertex.vid,
    );

    Box::new(coercion_iter.filter_map(
        |(ctx, can_coerce)| {
            if can_coerce {
                Some(ctx)
            } else {
                None
            }
        },
    ))
}

fn compute_component<'query, DataToken>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    mut iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>
where
    DataToken: Clone + Debug + 'query,
{
    let component_root_vid = component.root;
    let root_vertex = &component.vertices[&component_root_vid];

    iterator = coerce_if_needed(adapter.as_ref(), query, root_vertex, iterator);

    for filter_expr in &root_vertex.filters {
        iterator = apply_local_field_filter(
            adapter.as_ref(),
            query,
            component,
            component.root,
            filter_expr,
            iterator,
        );
    }

    iterator = Box::new(iterator.map(move |mut context| {
        context.record_token(component_root_vid);
        context
    }));

    let mut visited_vids: BTreeSet<Vid> = btreeset! {component_root_vid};

    let mut edge_iter = component.edges.values();
    let mut fold_iter = component.folds.values();
    let mut next_edge = edge_iter.next();
    let mut next_fold = fold_iter.next();
    loop {
        let (process_next_fold, process_next_edge) = match (next_fold, next_edge) {
            (None, None) => break,
            (None, Some(_)) | (Some(_), None) => (next_fold, next_edge),
            (Some(fold), Some(edge)) => match fold.eid.cmp(&edge.eid) {
                std::cmp::Ordering::Greater => (None, Some(edge)),
                std::cmp::Ordering::Less => (Some(fold), None),
                std::cmp::Ordering::Equal => unreachable!(),
            },
        };

        assert!(process_next_fold.is_some() ^ process_next_edge.is_some());

        if let Some(fold) = process_next_fold {
            let from_vid_unvisited = visited_vids.insert(fold.from_vid);
            let to_vid_unvisited = visited_vids.insert(fold.to_vid);
            assert!(!from_vid_unvisited);
            assert!(to_vid_unvisited);

            iterator = compute_fold(
                adapter.clone(),
                query,
                &component.vertices[&fold.from_vid],
                component,
                fold.clone(),
                iterator,
            );

            next_fold = fold_iter.next();
        } else if let Some(edge) = process_next_edge {
            let from_vid_unvisited = visited_vids.insert(edge.from_vid);
            let to_vid_unvisited = visited_vids.insert(edge.to_vid);
            assert!(!from_vid_unvisited);
            assert!(to_vid_unvisited);

            iterator = expand_edge(
                adapter.clone(),
                query,
                component,
                edge.from_vid,
                edge.to_vid,
                edge,
                iterator,
            );

            next_edge = edge_iter.next();
        }
    }

    iterator
}

fn construct_outputs<'query, DataToken: Clone + Debug + 'query>(
    adapter: &RefCell<impl Adapter<'query, DataToken = DataToken>>,
    query: &InterpretedQuery,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'query> {
    let ir_query = &query.indexed_query.ir_query;
    let mut output_names: Vec<Arc<str>> = ir_query.root_component.outputs.keys().cloned().collect();
    output_names.sort_unstable(); // to ensure deterministic project_property() ordering

    let mut output_iterator = iterator;

    for output_name in output_names.iter() {
        let context_field = &ir_query.root_component.outputs[output_name];
        let vertex_id = context_field.vertex_id;
        let moved_iterator = Box::new(output_iterator.map(move |context| {
            let new_token = context.tokens[&vertex_id].clone();
            context.move_to_token(new_token)
        }));

        let current_type_name = &ir_query.root_component.vertices[&vertex_id].type_name;
        let mut adapter_ref = adapter.borrow_mut();
        let field_data_iterator = adapter_ref.project_property(
            moved_iterator,
            current_type_name.clone(),
            context_field.field_name.clone(),
            query.clone(),
            vertex_id,
        );
        drop(adapter_ref);

        output_iterator = Box::new(field_data_iterator.map(|(mut context, value)| {
            context.values.push(value);
            context
        }));
    }

    let expected_output_names: BTreeSet<_> = query.indexed_query.outputs.keys().cloned().collect();

    Box::new(output_iterator.map(move |mut context| {
        assert!(context.values.len() == output_names.len());

        let mut output: BTreeMap<Arc<str>, FieldValue> = output_names
            .iter()
            .cloned()
            .zip(context.values.drain(..))
            .collect();

        for ((_, output_name), output_value) in context.folded_values {
            let existing = output.insert(output_name, output_value.into());
            assert!(existing.is_none());
        }

        debug_assert_eq!(expected_output_names, output.keys().cloned().collect());

        output
    }))
}

/// If this IRFold has a filter on the folded element count, and that filter imposes
/// a max size that can be statically determined, return that max size so it can
/// be used for further optimizations. Otherwise, return None.
fn get_max_fold_count_limit(query: &InterpretedQuery, fold: &IRFold) -> Option<usize> {
    let mut result: Option<usize> = None;

    for post_fold_filter in fold.post_filters.iter() {
        let next_limit = match post_fold_filter {
            Operation::Equals(FoldSpecificFieldKind::Count, Argument::Variable(var_ref))
            | Operation::LessThanOrEqual(
                FoldSpecificFieldKind::Count,
                Argument::Variable(var_ref),
            ) => {
                let variable_value = query.arguments[&var_ref.variable_name].as_usize().unwrap();
                Some(variable_value)
            }
            Operation::LessThan(FoldSpecificFieldKind::Count, Argument::Variable(var_ref)) => {
                let variable_value = query.arguments[&var_ref.variable_name].as_usize().unwrap();
                // saturating_sub() here is a safeguard against underflow: in principle,
                // we shouldn't see a comparison for "< 0", but if we do regardless, we'd prefer to
                // saturate to 0 rather than wrapping around. This check is an optimization and
                // is allowed to be more conservative than strictly necessary.
                // The later full application of filters ensures correctness.
                Some(variable_value.saturating_sub(1))
            }
            Operation::OneOf(FoldSpecificFieldKind::Count, Argument::Variable(var_ref)) => {
                match &query.arguments[&var_ref.variable_name] {
                    FieldValue::List(v) => v.iter().map(|x| x.as_usize().unwrap()).max(),
                    _ => unreachable!(),
                }
            }
            _ => None,
        };

        match (result, next_limit) {
            (None, _) => result = next_limit,
            (Some(l), Some(r)) if l > r => result = next_limit,
            _ => {}
        }
    }

    result
}

fn collect_fold_elements<'query, DataToken: Clone + Debug + 'query>(
    mut iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
    max_fold_count_limit: &Option<usize>,
) -> Option<Vec<DataContext<DataToken>>> {
    if let Some(max_fold_count_limit) = max_fold_count_limit {
        // If this fold has more than `max_fold_count_limit` elements,
        // it will get filtered out by a post-fold filter.
        // Pulling elements from `iterator` causes computations and data fetches to happen,
        // and as an optimization we'd like to stop pulling elements as soon as possible.
        // If we are able to pull more than `max_fold_count_limit + 1` elements,
        // we know that this fold is going to get filtered out, so we might as well
        // stop materializing its elements early.
        let mut fold_elements = Vec::with_capacity(*max_fold_count_limit);

        let mut stopped_early = false;
        for _ in 0..*max_fold_count_limit {
            if let Some(element) = iterator.next() {
                fold_elements.push(element);
            } else {
                stopped_early = true;
                break;
            }
        }

        if !stopped_early && iterator.next().is_some() {
            // There are more elements than the max size allowed by the filters on this fold.
            // It's going to get filtered out anyway, so we can avoid materializing the rest.
            return None;
        }

        Some(fold_elements)
    } else {
        // We weren't able to find any early-termination condition for materializing the fold,
        // so materialize the whole thing and return it.
        Some(iterator.collect())
    }
}

#[allow(unused_variables)]
fn compute_fold<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    expanding_from: &IRVertex,
    parent_component: &IRQueryComponent,
    fold: Arc<IRFold>,
    mut iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let mut adapter_ref = adapter.borrow_mut();

    // Get any imported tag values needed inside the fold component or one of its subcomponents.
    for imported_field in fold.imported_tags.iter().cloned() {
        let activated_vertex_iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
            Box::new(iterator.map(move |x| x.activate_token(&imported_field.vertex_id)));

        let field_vertex = &parent_component.vertices[&imported_field.vertex_id];
        let current_type_name = &field_vertex.type_name;
        let context_and_value_iterator = adapter_ref.project_property(
            activated_vertex_iterator,
            current_type_name.clone(),
            imported_field.field_name.clone(),
            query.clone(),
            imported_field.vertex_id,
        );

        iterator = Box::new(context_and_value_iterator.map(move |(mut context, value)| {
            context.imported_tags.insert(
                (imported_field.vertex_id, imported_field.field_name.clone()),
                value,
            );
            context
        }))
    }

    // Get the initial vertices inside the folded scope.
    let expanding_from_vid = expanding_from.vid;
    let activated_vertex_iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
        Box::new(iterator.map(move |x| x.activate_token(&expanding_from_vid)));
    let current_type_name = &expanding_from.type_name;
    let edge_iterator = adapter_ref.project_neighbors(
        activated_vertex_iterator,
        current_type_name.clone(),
        fold.edge_name.clone(),
        fold.parameters.clone(),
        query.clone(),
        expanding_from.vid,
        fold.eid,
    );
    drop(adapter_ref);

    // Materialize the full fold data.
    // These values are moved into the closure.
    let cloned_adapter = adapter.clone();
    let cloned_query = query.clone();
    let fold_component = fold.component.clone();
    let fold_eid = fold.eid;
    let max_fold_size = get_max_fold_count_limit(query, fold.as_ref());
    let moved_fold = fold.clone();
    let folded_iterator = edge_iterator.filter_map(move |(mut context, neighbors)| {
        let imported_tags = context.imported_tags.clone();

        let neighbor_contexts = Box::new(neighbors.map(move |x| {
            let mut ctx = DataContext::new(Some(x));
            ctx.imported_tags = imported_tags.clone();
            ctx
        }));

        let computed_iterator = compute_component(
            cloned_adapter.clone(),
            &cloned_query,
            &fold_component,
            neighbor_contexts,
        );

        let fold_elements = match collect_fold_elements(computed_iterator, &max_fold_size) {
            None => {
                // We were able to discard this fold early.
                return None;
            }
            Some(f) => f,
        };
        context
            .folded_contexts
            .insert_or_error(fold_eid, fold_elements)
            .unwrap();

        // Remove no-longer-needed imported tags.
        for imported_tag in &moved_fold.imported_tags {
            context
                .imported_tags
                .remove(&(imported_tag.vertex_id, imported_tag.field_name.clone()))
                .unwrap();
        }

        Some(context)
    });

    // Apply post-fold filters.
    let mut post_filtered_iterator: Box<dyn Iterator<Item = DataContext<DataToken>>> =
        Box::new(folded_iterator);
    let adapter_ref = adapter.as_ref();
    for post_fold_filter in fold.post_filters.iter() {
        post_filtered_iterator = apply_fold_specific_filter(
            adapter_ref,
            query,
            fold.component.as_ref(),
            fold.as_ref(),
            expanding_from.vid,
            post_fold_filter,
            post_filtered_iterator,
        );
    }

    // Compute the outputs from this fold.
    let mut output_names: Vec<Arc<str>> = fold.component.outputs.keys().cloned().collect();
    output_names.sort_unstable(); // to ensure deterministic project_property() ordering

    let cloned_adapter = adapter.clone();
    let cloned_query = query.clone();
    let final_iterator = post_filtered_iterator.map(move |mut ctx| {
        let fold_elements = ctx.folded_contexts.remove(&fold_eid).unwrap();

        // Add any fold-specific field outputs to the context's folded values.
        for (output_name, fold_specific_field) in &fold.fold_specific_outputs {
            let value = match fold_specific_field {
                FoldSpecificFieldKind::Count => {
                    ValueOrVec::Value(FieldValue::Uint64(fold_elements.len() as u64))
                }
            };
            ctx.folded_values
                .insert_or_error((fold_eid, output_name.clone()), value)
                .unwrap();
        }

        let mut output_iterator: Box<dyn Iterator<Item = DataContext<DataToken>>> =
            Box::new(fold_elements.into_iter());
        for output_name in output_names.iter() {
            let context_field = &fold.component.outputs[output_name.as_ref()];
            let vertex_id = context_field.vertex_id;
            let moved_iterator = Box::new(output_iterator.map(move |context| {
                let new_token = context.tokens[&vertex_id].clone();
                context.move_to_token(new_token)
            }));

            let mut adapter_ref = cloned_adapter.borrow_mut();
            let field_data_iterator = adapter_ref.project_property(
                moved_iterator,
                fold.component.vertices[&vertex_id].type_name.clone(),
                context_field.field_name.clone(),
                cloned_query.clone(),
                vertex_id,
            );
            drop(adapter_ref);

            output_iterator = Box::new(field_data_iterator.map(|(mut context, value)| {
                context.values.push(value);
                context
            }));
        }

        let mut folded_values: BTreeMap<(Eid, Arc<str>), ValueOrVec> = output_names
            .iter()
            .map(|output| ((fold_eid, output.clone()), ValueOrVec::Vec(vec![])))
            .collect();
        for mut folded_context in output_iterator {
            for (key, value) in folded_context.folded_values {
                folded_values
                    .entry(key)
                    .or_insert_with(|| ValueOrVec::Vec(vec![]))
                    .as_mut_vec()
                    .unwrap()
                    .push(value);
            }

            // We pushed values onto folded_context.values with output names in increasing order
            // and we are now popping from the back. That means we're getting the highest name
            // first, so we should reverse our output_names iteration order.
            for output in output_names.iter().rev() {
                let value = folded_context.values.pop().unwrap();
                folded_values
                    .get_mut(&(fold_eid, output.clone()))
                    .unwrap()
                    .as_mut_vec()
                    .unwrap()
                    .push(ValueOrVec::Value(value));
            }
        }

        let prior_folded_values_count = ctx.folded_values.len();
        let new_folded_values_count = folded_values.len();
        ctx.folded_values.extend(folded_values.into_iter());

        // Ensure the merged maps had disjoint keys.
        assert_eq!(
            ctx.folded_values.len(),
            prior_folded_values_count + new_folded_values_count
        );

        ctx
    });

    Box::new(final_iterator)
}

/// Check whether a tagged value that is being used in a filter originates from
/// a scope that is optional and missing, and therefore the filter should pass.
///
/// A small subtlety is important here: it's possible that the tagged value is *local* to
/// the scope being filtered. In that case, the context *will not* yet have a token associated
/// with the vertex ID of the tag's ContextField. However, in such cases, the tagged value
/// is *never* optional relative to the current scope, so we can safely return `false`.
#[inline(always)]
fn is_tag_optional_and_missing<'query, DataToken: Clone + Debug + 'query>(
    context: &DataContext<DataToken>,
    tagged_field: &FieldRef,
) -> bool {
    // Get a representative Vid that will show whether the tagged value exists or not.
    let vid = match tagged_field {
        FieldRef::ContextField(field) => field.vertex_id,
        FieldRef::FoldSpecificField(field) => field.fold_root_vid,
    };

    // Some(None) means "there's a value associated with that Vid, and it's None".
    // None would mean that the tagged value is local, i.e. nothing is associated with that Vid yet.
    // Some(Some(token)) would mean that a vertex was found and associated with that Vid.
    matches!(context.tokens.get(&vid), Some(None))
}

macro_rules! implement_filter {
    ( $iter: ident, $right: ident, $func: ident ) => {
        Box::new($iter.filter_map(move |mut context| {
            let right_value = context.values.pop().unwrap();
            let left_value = context.values.pop().unwrap();
            if let Argument::Tag(field) = &$right {
                if is_tag_optional_and_missing(&context, field) {
                    return Some(context);
                }
            }

            if $func(&left_value, &right_value) {
                Some(context)
            } else {
                None
            }
        }))
    };
}

macro_rules! implement_negated_filter {
    ( $iter: ident, $right: ident, $func: ident ) => {
        Box::new($iter.filter_map(move |mut context| {
            let right_value = context.values.pop().unwrap();
            let left_value = context.values.pop().unwrap();
            if let Argument::Tag(field) = &$right {
                if is_tag_optional_and_missing(&context, field) {
                    return Some(context);
                }
            }

            if $func(&left_value, &right_value) {
                None
            } else {
                Some(context)
            }
        }))
    };
}

fn apply_local_field_filter<'query, DataToken: Clone + Debug + 'query>(
    adapter_ref: &RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    current_vid: Vid,
    filter: &Operation<LocalField, Argument>,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let local_field = filter.left();
    let field_iterator = compute_local_field(
        adapter_ref,
        query,
        component,
        current_vid,
        local_field,
        iterator,
    );

    apply_filter(
        adapter_ref,
        query,
        component,
        current_vid,
        filter,
        field_iterator,
    )
}

fn apply_fold_specific_filter<'query, DataToken: Clone + Debug + 'query>(
    adapter_ref: &RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    fold: &IRFold,
    current_vid: Vid,
    filter: &Operation<FoldSpecificFieldKind, Argument>,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let fold_specific_field = filter.left();
    let field_iterator = compute_fold_specific_field(fold, fold_specific_field, iterator);

    apply_filter(
        adapter_ref,
        query,
        component,
        current_vid,
        filter,
        field_iterator,
    )
}

fn apply_filter<
    'query,
    DataToken: Clone + Debug + 'query,
    LeftT: Debug + Clone + PartialEq + Eq,
>(
    adapter_ref: &RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    current_vid: Vid,
    filter: &Operation<LeftT, Argument>,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let expression_iterator = match filter.right() {
        Some(Argument::Tag(FieldRef::ContextField(context_field))) => {
            if context_field.vertex_id == current_vid {
                // This tag is from the vertex we're currently filtering. That means the field
                // whose value we want to get is actually local, so there's no need to compute it
                // using the more expensive approach we use for non-local fields.
                let local_equivalent_field = LocalField {
                    field_name: context_field.field_name.clone(),
                    field_type: context_field.field_type.clone(),
                };
                compute_local_field(
                    adapter_ref,
                    query,
                    component,
                    current_vid,
                    &local_equivalent_field,
                    iterator,
                )
            } else {
                compute_context_field(adapter_ref, query, component, context_field, iterator)
            }
        }
        Some(Argument::Tag(FieldRef::FoldSpecificField(_fold_field))) => {
            todo!()
        }
        Some(Argument::Variable(var)) => {
            let right_value = query.arguments[var.variable_name.as_ref()].to_owned();
            Box::new(iterator.map(move |mut ctx| {
                // TODO: implement more efficient filtering with:
                //       - no clone of runtime parameter values
                //       - omit the "tag from missing optional" check if the filter argument isn't
                //         a tag, or if it's a tag that isn't from an optional scope relative to
                //         the current scope
                //       - type awareness: we know the type of the field being filtered,
                //         and we probably know (or can infer) the type of the filtering argument(s)
                //       - precomputation to improve efficiency: build regexes once,
                //         turn "in_collection" filter arguments into sets if possible, etc.
                ctx.values.push(right_value.to_owned());
                ctx
            }))
        }
        None => iterator,
    };

    match filter.clone() {
        Operation::IsNull(_) => {
            let output_iter = expression_iterator.filter_map(move |mut context| {
                let last_value = context.values.pop().unwrap();
                match last_value {
                    FieldValue::Null => Some(context),
                    _ => None,
                }
            });
            Box::new(output_iter)
        }
        Operation::IsNotNull(_) => {
            let output_iter = expression_iterator.filter_map(move |mut context| {
                let last_value = context.values.pop().unwrap();
                match last_value {
                    FieldValue::Null => None,
                    _ => Some(context),
                }
            });
            Box::new(output_iter)
        }
        Operation::Equals(_, right) => {
            implement_filter!(expression_iterator, right, equals)
        }
        Operation::NotEquals(_, right) => {
            implement_negated_filter!(expression_iterator, right, equals)
        }
        Operation::GreaterThan(_, right) => {
            implement_filter!(expression_iterator, right, greater_than)
        }
        Operation::GreaterThanOrEqual(_, right) => {
            implement_filter!(expression_iterator, right, greater_than_or_equal)
        }
        Operation::LessThan(_, right) => {
            implement_filter!(expression_iterator, right, less_than)
        }
        Operation::LessThanOrEqual(_, right) => {
            implement_filter!(expression_iterator, right, less_than_or_equal)
        }
        Operation::HasSubstring(_, right) => {
            implement_filter!(expression_iterator, right, has_substring)
        }
        Operation::NotHasSubstring(_, right) => {
            implement_negated_filter!(expression_iterator, right, has_substring)
        }
        Operation::OneOf(_, right) => {
            implement_filter!(expression_iterator, right, one_of)
        }
        Operation::NotOneOf(_, right) => {
            implement_negated_filter!(expression_iterator, right, one_of)
        }
        Operation::Contains(_, right) => {
            implement_filter!(expression_iterator, right, contains)
        }
        Operation::NotContains(_, right) => {
            implement_negated_filter!(expression_iterator, right, contains)
        }
        Operation::HasPrefix(_, right) => {
            implement_filter!(expression_iterator, right, has_prefix)
        }
        Operation::NotHasPrefix(_, right) => {
            implement_negated_filter!(expression_iterator, right, has_prefix)
        }
        Operation::HasSuffix(_, right) => {
            implement_filter!(expression_iterator, right, has_suffix)
        }
        Operation::NotHasSuffix(_, right) => {
            implement_negated_filter!(expression_iterator, right, has_suffix)
        }
        Operation::RegexMatches(_, right) => match &right {
            Argument::Tag(_) => {
                implement_filter!(expression_iterator, right, regex_matches_slow_path)
            }
            Argument::Variable(var) => {
                let variable_value = &query.arguments[var.variable_name.as_ref()];
                let pattern = Regex::new(variable_value.as_str().unwrap()).unwrap();

                Box::new(expression_iterator.filter_map(move |mut context| {
                    let _ = context.values.pop().unwrap();
                    let left_value = context.values.pop().unwrap();

                    if regex_matches_optimized(&left_value, &pattern) {
                        Some(context)
                    } else {
                        None
                    }
                }))
            }
        },
        Operation::NotRegexMatches(_, right) => match &right {
            Argument::Tag(_) => {
                implement_negated_filter!(expression_iterator, right, regex_matches_slow_path)
            }
            Argument::Variable(var) => {
                let variable_value = &query.arguments[var.variable_name.as_ref()];
                let pattern = Regex::new(variable_value.as_str().unwrap()).unwrap();

                Box::new(expression_iterator.filter_map(move |mut context| {
                    let _ = context.values.pop().unwrap();
                    let left_value = context.values.pop().unwrap();

                    if !regex_matches_optimized(&left_value, &pattern) {
                        Some(context)
                    } else {
                        None
                    }
                }))
            }
        },
    }
}

fn compute_context_field<'query, DataToken: Clone + Debug + 'query>(
    adapter: &RefCell<impl Adapter<'query, DataToken = DataToken>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    context_field: &ContextField,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let vertex_id = context_field.vertex_id;

    if let Some(vertex) = component.vertices.get(&vertex_id) {
        let moved_iterator = iterator.map(move |mut context| {
            let current_token = context.current_token.clone();
            let new_token = context.tokens[&vertex_id].clone();
            context.suspended_tokens.push(current_token);
            context.move_to_token(new_token)
        });

        let current_type_name = &vertex.type_name;
        let mut adapter_ref = adapter.borrow_mut();
        let context_and_value_iterator = adapter_ref.project_property(
            Box::new(moved_iterator),
            current_type_name.clone(),
            context_field.field_name.clone(),
            query.clone(),
            vertex_id,
        );
        drop(adapter_ref);

        Box::new(context_and_value_iterator.map(|(mut context, value)| {
            context.values.push(value);

            // Make sure that the context has the same "current" token
            // as before evaluating the context field.
            let old_current_token = context.suspended_tokens.pop().unwrap();
            context.move_to_token(old_current_token)
        }))
    } else {
        // This context field represents an imported tag value from an outer component.
        // Grab its value from the context itself.
        let field_name = context_field.field_name.clone();
        let key = (vertex_id, field_name);
        Box::new(iterator.map(move |mut context| {
            let value = context.imported_tags[&key].clone();
            context.values.push(value);

            context
        }))
    }
}

fn compute_fold_specific_field<'query, DataToken: Clone + Debug + 'query>(
    fold: &IRFold,
    fold_specific_field: &FoldSpecificFieldKind,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let fold_eid = fold.eid;
    match fold_specific_field {
        FoldSpecificFieldKind::Count => Box::new(iterator.map(move |mut ctx| {
            let value = ctx.folded_contexts[&fold_eid].len();
            ctx.values.push(FieldValue::Uint64(value as u64));
            ctx
        })),
    }
}

fn compute_local_field<'query, DataToken: Clone + Debug + 'query>(
    adapter: &RefCell<impl Adapter<'query, DataToken = DataToken>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    current_vid: Vid,
    local_field: &LocalField,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let current_type_name = &component.vertices[&current_vid].type_name;
    let mut adapter_ref = adapter.borrow_mut();
    let context_and_value_iterator = adapter_ref.project_property(
        iterator,
        current_type_name.clone(),
        local_field.field_name.clone(),
        query.clone(),
        current_vid,
    );
    drop(adapter_ref);

    Box::new(context_and_value_iterator.map(|(mut context, value)| {
        context.values.push(value);
        context
    }))
}

struct EdgeExpander<'query, DataToken: Clone + Debug + 'query> {
    context: DataContext<DataToken>,
    neighbor_tokens: Box<dyn Iterator<Item = DataToken> + 'query>,
    is_optional_edge: bool,
    has_neighbors: bool,
    neighbors_ended: bool,
    ended: bool,
}

impl<'query, DataToken: Clone + Debug + 'query> EdgeExpander<'query, DataToken> {
    pub fn new(
        context: DataContext<DataToken>,
        neighbor_tokens: Box<dyn Iterator<Item = DataToken> + 'query>,
        is_optional_edge: bool,
    ) -> EdgeExpander<'query, DataToken> {
        EdgeExpander {
            context,
            neighbor_tokens,
            is_optional_edge,
            has_neighbors: false,
            neighbors_ended: false,
            ended: false,
        }
    }
}

impl<'query, DataToken: Clone + Debug + 'query> Iterator for EdgeExpander<'query, DataToken> {
    type Item = DataContext<DataToken>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }

        if !self.neighbors_ended {
            let neighbor = self.neighbor_tokens.next();
            if neighbor.is_some() {
                self.has_neighbors = true;
                return Some(self.context.split_and_move_to_token(neighbor));
            } else {
                self.neighbors_ended = true;
            }
        }

        assert!(self.neighbors_ended);
        self.ended = true;

        // If there's no current token, there couldn't possibly be neighbors.
        // If this assertion trips, the adapter's project_neighbors() implementation illegally
        // returned neighbors for a non-existent vertex.
        if self.context.current_token.is_none() {
            assert!(!self.has_neighbors);
        }

        // If the current token is None, that means that a prior edge was optional and missing.
        // In that case, we couldn't possibly have found any neighbors here, but the optional-ness
        // of that prior edge means we have to return a context with no active token.
        //
        // The other case where we have to return a context with no active token is when
        // we have a current token, but the edge we're traversing is optional and does not exist.
        if self.context.current_token.is_none() || (!self.has_neighbors && self.is_optional_edge) {
            Some(self.context.split_and_move_to_token(None))
        } else {
            None
        }
    }
}

fn expand_edge<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    expanding_from_vid: Vid,
    expanding_to_vid: Vid,
    edge: &IREdge,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let expanded_iterator = if let Some(recursive) = &edge.recursive {
        expand_recursive_edge(
            adapter.clone(),
            query,
            component,
            &component.vertices[&expanding_from_vid],
            &component.vertices[&expanding_to_vid],
            edge.eid,
            &edge.edge_name,
            &edge.parameters,
            recursive,
            iterator,
        )
    } else {
        expand_non_recursive_edge(
            adapter.clone(),
            query,
            component,
            &component.vertices[&expanding_from_vid],
            &component.vertices[&expanding_to_vid],
            edge.eid,
            &edge.edge_name,
            &edge.parameters,
            edge.optional,
            iterator,
        )
    };

    perform_entry_into_new_vertex(
        adapter,
        query,
        component,
        &component.vertices[&expanding_to_vid],
        expanded_iterator,
    )
}

#[allow(clippy::too_many_arguments)]
fn expand_non_recursive_edge<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    _component: &IRQueryComponent,
    expanding_from: &IRVertex,
    _expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &Option<Arc<EdgeParameters>>,
    is_optional: bool,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let expanding_from_vid = expanding_from.vid;
    let expanding_vertex_iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
        Box::new(iterator.map(move |x| x.activate_token(&expanding_from_vid)));

    let current_type_name = &expanding_from.type_name;
    let mut adapter_ref = adapter.borrow_mut();
    let edge_iterator = adapter_ref.project_neighbors(
        expanding_vertex_iterator,
        current_type_name.clone(),
        edge_name.clone(),
        edge_parameters.clone(),
        query.clone(),
        expanding_from.vid,
        edge_id,
    );
    drop(adapter_ref);

    Box::new(edge_iterator.flat_map(move |(context, neighbor_iterator)| {
        EdgeExpander::new(context, neighbor_iterator, is_optional)
    }))
}

/// Apply all the operations needed at entry into a new vertex:
/// - coerce the type, if needed
/// - apply all local filters
/// - record the token at this Vid in the context
fn perform_entry_into_new_vertex<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    vertex: &IRVertex,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let vertex_id = vertex.vid;
    let mut iterator = coerce_if_needed(adapter.as_ref(), query, vertex, iterator);
    for filter_expr in vertex.filters.iter() {
        iterator = apply_local_field_filter(
            adapter.as_ref(),
            query,
            component,
            vertex_id,
            filter_expr,
            iterator,
        );
    }
    Box::new(iterator.map(move |mut x| {
        x.record_token(vertex_id);
        x
    }))
}

#[allow(clippy::too_many_arguments)]
fn expand_recursive_edge<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &Option<Arc<EdgeParameters>>,
    recursive: &Recursive,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let expanding_from_vid = expanding_from.vid;
    let mut recursion_iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
        Box::new(iterator.map(move |mut context| {
            if context.current_token.is_none() {
                // Mark that this token starts off with a None current_token value,
                // so the later unsuspend() call should restore it to such a state later.
                context.suspended_tokens.push(None);
            }
            context.activate_token(&expanding_from_vid)
        }));

    let max_depth = usize::from(recursive.depth);
    recursion_iterator = perform_one_recursive_edge_expansion(
        adapter.clone(),
        query,
        component,
        expanding_from.type_name.clone(),
        expanding_from,
        expanding_to,
        edge_id,
        edge_name,
        edge_parameters,
        recursion_iterator,
    );

    let edge_endpoint_type = expanding_to
        .coerced_from_type
        .as_ref()
        .unwrap_or(&expanding_to.type_name);
    let recursing_from = recursive.coerce_to.as_ref().unwrap_or(edge_endpoint_type);

    for _ in 2..=max_depth {
        if let Some(coerce_to) = recursive.coerce_to.as_ref() {
            let mut adapter_ref = adapter.borrow_mut();
            let coercion_iter = adapter_ref.can_coerce_to_type(
                recursion_iterator,
                edge_endpoint_type.clone(),
                coerce_to.clone(),
                query.clone(),
                expanding_from_vid,
            );

            // This coercion is unusual since it doesn't discard elements that can't be coerced.
            // This is because we still want to produce those elements, and we simply want to
            // not continue recursing deeper through them since they don't have the edge we need.
            recursion_iterator = Box::new(coercion_iter.map(|(ctx, can_coerce)| {
                if can_coerce {
                    ctx
                } else {
                    ctx.ensure_suspended()
                }
            }));
        }

        recursion_iterator = perform_one_recursive_edge_expansion(
            adapter.clone(),
            query,
            component,
            recursing_from.clone(),
            expanding_from,
            expanding_to,
            edge_id,
            edge_name,
            edge_parameters,
            recursion_iterator,
        );
    }

    post_process_recursive_expansion(recursion_iterator)
}

#[allow(clippy::too_many_arguments)]
fn perform_one_recursive_edge_expansion<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    _component: &IRQueryComponent,
    expanding_from_type: Arc<str>,
    expanding_from: &IRVertex,
    _expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &Option<Arc<EdgeParameters>>,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let mut adapter_ref = adapter.borrow_mut();
    let edge_iterator = adapter_ref.project_neighbors(
        iterator,
        expanding_from_type,
        edge_name.clone(),
        edge_parameters.clone(),
        query.clone(),
        expanding_from.vid,
        edge_id,
    );
    drop(adapter_ref);

    let result_iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
        Box::new(edge_iterator.flat_map(move |(context, neighbor_iterator)| {
            RecursiveEdgeExpander::new(context, neighbor_iterator)
        }));

    result_iterator
}

struct RecursiveEdgeExpander<'query, DataToken: Clone + Debug + 'query> {
    context: Option<DataContext<DataToken>>,
    neighbor_base: Option<DataContext<DataToken>>,
    neighbor_tokens: Box<dyn Iterator<Item = DataToken> + 'query>,
    has_neighbors: bool,
    neighbors_ended: bool,
}

impl<'query, DataToken: Clone + Debug + 'query> RecursiveEdgeExpander<'query, DataToken> {
    pub fn new(
        context: DataContext<DataToken>,
        neighbor_tokens: Box<dyn Iterator<Item = DataToken> + 'query>,
    ) -> RecursiveEdgeExpander<'query, DataToken> {
        RecursiveEdgeExpander {
            context: Some(context),
            neighbor_base: None,
            neighbor_tokens,
            has_neighbors: false,
            neighbors_ended: false,
        }
    }
}

impl<'query, DataToken: Clone + Debug + 'query> Iterator
    for RecursiveEdgeExpander<'query, DataToken>
{
    type Item = DataContext<DataToken>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.neighbors_ended {
            let neighbor = self.neighbor_tokens.next();

            if let Some(token) = neighbor {
                if let Some(context) = self.context.take() {
                    // Prep a neighbor base context for future use, since we're moving
                    // the "self" context out.
                    self.neighbor_base = Some(context.split_and_move_to_token(None));

                    // Attach the "self" context as a piggyback rider on the neighbor.
                    let mut neighbor_context = context.split_and_move_to_token(Some(token));
                    neighbor_context
                        .piggyback
                        .get_or_insert_with(Default::default)
                        .push(context.ensure_suspended());
                    return Some(neighbor_context);
                } else {
                    // The "self" token has already been moved out, so use the neighbor base context
                    // as the starting point for constructing a new context.
                    return Some(
                        self.neighbor_base
                            .as_ref()
                            .unwrap()
                            .split_and_move_to_token(Some(token)),
                    );
                }
            } else {
                self.neighbors_ended = true;

                // If there's no current token, there couldn't possibly be neighbors.
                // If this assertion trips, the adapter's project_neighbors() implementation
                // illegally returned neighbors for a non-existent vertex.
                if let Some(context) = &self.context {
                    if context.current_token.is_none() {
                        assert!(!self.has_neighbors);
                    }
                }
            }
        }

        self.context.take()
    }
}

fn unpack_piggyback<DataToken: Debug + Clone>(
    context: DataContext<DataToken>,
) -> Vec<DataContext<DataToken>> {
    let mut result = Default::default();

    unpack_piggyback_inner(&mut result, context);

    result
}

fn unpack_piggyback_inner<DataToken: Debug + Clone>(
    output: &mut Vec<DataContext<DataToken>>,
    mut context: DataContext<DataToken>,
) {
    if let Some(mut piggyback) = context.piggyback.take() {
        for ctx in piggyback.drain(..) {
            unpack_piggyback_inner(output, ctx);
        }
    }

    output.push(context);
}

fn post_process_recursive_expansion<'query, DataToken: Clone + Debug + 'query>(
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    Box::new(
        iterator
            .flat_map(|context| unpack_piggyback(context))
            .map(|context| {
                assert!(context.piggyback.is_none());
                context.ensure_unsuspended()
            }),
    )
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use trustfall_filetests_macros::parameterize;

    use crate::{
        interpreter::{error::QueryArgumentsError, InterpretedQuery},
        ir::{indexed::IndexedQuery, FieldValue},
        util::TestIRQueryResult,
    };

    #[parameterize("trustfall_core/src/resources/test_data/execution_errors")]
    fn parameterizable_tester(base: &Path, stem: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{}.ir.ron", stem));

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{}{}", stem, ".exec-error.ron"));
        let check_data = fs::read_to_string(check_path).unwrap();

        let input_data = fs::read_to_string(input_path).unwrap();
        let test_query: TestIRQueryResult = ron::from_str(&input_data).unwrap();
        let test_query = test_query.unwrap();

        let arguments: BTreeMap<Arc<str>, FieldValue> = test_query
            .arguments
            .into_iter()
            .map(|(k, v)| (Arc::from(k), v))
            .collect();

        let indexed_query: IndexedQuery = test_query.ir_query.try_into().unwrap();
        let constructed_test_item = InterpretedQuery::from_query_and_arguments(
            Arc::from(indexed_query),
            Arc::from(arguments),
        );

        let check_parsed: Result<_, QueryArgumentsError> = Err(ron::from_str(&check_data).unwrap());

        assert_eq!(check_parsed, constructed_test_item);
    }
}
