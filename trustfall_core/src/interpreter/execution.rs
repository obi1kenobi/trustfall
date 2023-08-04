use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    sync::Arc,
};

use crate::{
    ir::{
        Argument, ContextField, EdgeParameters, Eid, FieldRef, FieldValue, FoldSpecificFieldKind,
        IREdge, IRFold, IRQueryComponent, IRVertex, IndexedQuery, LocalField, Operation, Recursive,
        Vid,
    },
    util::BTreeMapTryInsertExt,
};

use super::{
    error::QueryArgumentsError, filtering::apply_filter, Adapter, ContextIterator,
    ContextOutcomeIterator, DataContext, InterpretedQuery, ResolveEdgeInfo, ResolveInfo,
    TaggedValue, ValueOrVec, VertexIterator,
};

#[derive(Debug, Clone)]
pub(super) struct QueryCarrier {
    pub(in crate::interpreter) query: Option<InterpretedQuery>,
}

#[allow(clippy::type_complexity)]
pub fn interpret_ir<'query, AdapterT: Adapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    indexed_query: Arc<IndexedQuery>,
    arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
) -> Result<Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'query>, QueryArgumentsError>
{
    let query = InterpretedQuery::from_query_and_arguments(indexed_query, arguments)?;
    let root_vid = query.indexed_query.ir_query.root_component.root;

    let ir_query = &query.indexed_query.ir_query;
    let root_edge = &ir_query.root_name;
    let root_edge_parameters = &ir_query.root_parameters;

    let mut carrier = QueryCarrier { query: None };

    let resolve_info = ResolveInfo::new(query.clone(), root_vid, false);

    let mut iterator: ContextIterator<'query, AdapterT::Vertex> = Box::new(
        adapter
            .resolve_starting_vertices(root_edge, root_edge_parameters, &resolve_info)
            .map(|x| DataContext::new(Some(x))),
    );
    carrier.query = Some(resolve_info.into_inner());

    let component = &ir_query.root_component;
    iterator = compute_component(adapter.clone(), &mut carrier, component, iterator);

    Ok(construct_outputs(adapter.as_ref(), &mut carrier, iterator))
}

fn coerce_if_needed<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    vertex: &IRVertex,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    match vertex.coerced_from_type.as_ref() {
        None => iterator,
        Some(coerced_from) => {
            perform_coercion(adapter, carrier, vertex, coerced_from, &vertex.type_name, iterator)
        }
    }
}

fn perform_coercion<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    vertex: &IRVertex,
    coerced_from: &Arc<str>,
    coerce_to: &Arc<str>,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveInfo::new(query, vertex.vid, false);
    let coercion_iter = adapter.resolve_coercion(iterator, coerced_from, coerce_to, &resolve_info);
    carrier.query = Some(resolve_info.into_inner());

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

fn compute_component<'query, AdapterT: Adapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    mut iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let component_root_vid = component.root;
    let root_vertex = &component.vertices[&component_root_vid];

    iterator = coerce_if_needed(adapter.as_ref(), carrier, root_vertex, iterator);

    for filter_expr in &root_vertex.filters {
        iterator = apply_local_field_filter(
            adapter.as_ref(),
            carrier,
            component,
            component.root,
            filter_expr,
            iterator,
        );
    }

    iterator = Box::new(iterator.map(move |mut context| {
        context.record_vertex(component_root_vid);
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
                carrier,
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
                adapter.as_ref(),
                carrier,
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

fn construct_outputs<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> Box<dyn Iterator<Item = BTreeMap<Arc<str>, FieldValue>> + 'query> {
    let mut query = carrier.query.take().expect("query was not returned");

    let root_component = query.indexed_query.ir_query.root_component.clone();
    let mut output_names: Vec<Arc<str>> =
        query.indexed_query.ir_query.root_component.outputs.keys().cloned().collect();
    output_names.sort_unstable(); // to ensure deterministic resolve_property() ordering

    let mut output_iterator = iterator;

    for output_name in output_names.iter() {
        let context_field = &root_component.outputs[output_name];
        let vertex_id = context_field.vertex_id;

        let moved_iterator = Box::new(output_iterator.map(move |context| {
            let new_vertex = context.vertices[&vertex_id].clone();
            context.move_to_vertex(new_vertex)
        }));

        let resolve_info = ResolveInfo::new(query, vertex_id, true);

        let type_name = &root_component.vertices[&vertex_id].type_name;
        let field_data_iterator = adapter.resolve_property(
            moved_iterator,
            type_name,
            &context_field.field_name,
            &resolve_info,
        );
        query = resolve_info.into_inner();

        output_iterator = Box::new(field_data_iterator.map(|(mut context, value)| {
            context.values.push(value);
            context
        }));
    }
    let expected_output_names: BTreeSet<_> = query.indexed_query.outputs.keys().cloned().collect();
    carrier.query = Some(query);

    Box::new(output_iterator.map(move |mut context| {
        assert!(
            context.values.len() == output_names.len(),
            "expected {output_names:?} but got {:?}",
            &context.values
        );

        let mut output: BTreeMap<Arc<str>, FieldValue> =
            output_names.iter().cloned().zip(context.values.drain(..)).collect();

        for ((_, output_name), output_value) in context.folded_values {
            let existing = output.insert(output_name, output_value.into());
            assert!(existing.is_none());
        }

        debug_assert_eq!(expected_output_names, output.keys().cloned().collect());

        output
    }))
}

/// Extracts numeric [`FieldValue`] into a `usize`, clamping negative numbers to 0.
/// Returns `None` on `FieldValue::Null`, and panics otherwise.
fn usize_from_field_value(field_value: &FieldValue) -> Option<usize> {
    match field_value {
        FieldValue::Int64(num) => {
            Some(usize::try_from(*num.max(&0)).expect("i64 can be converted to usize"))
        }
        FieldValue::Uint64(num) => {
            Some(usize::try_from(*num).expect("i64 can be converted to usize"))
        }
        FieldValue::Null => None,
        FieldValue::Float64(_)
        | FieldValue::List(_)
        | FieldValue::Enum(_)
        | FieldValue::Boolean(_)
        | FieldValue::String(_) => {
            panic!("got field value {field_value:#?} in usize_from_field_value which should only ever get Int64, Uint64, or Null")
        }
    }
}

/// If this IRFold has a filter on the folded element count, and that filter imposes
/// a max size that can be statically determined, return that max size so it can
/// be used for further optimizations. Otherwise, return None.
fn get_max_fold_count_limit(carrier: &mut QueryCarrier, fold: &IRFold) -> Option<usize> {
    let mut result: Option<usize> = None;

    let query_arguments = &carrier.query.as_ref().expect("query was not returned").arguments;
    for post_fold_filter in fold.post_filters.iter() {
        let next_limit = match post_fold_filter {
            // Equals and OneOf must be visited here as they are not visited
            // in `get_min_fold_count_limit`
            Operation::Equals(FoldSpecificFieldKind::Count, Argument::Variable(var_ref))
            | Operation::LessThanOrEqual(
                FoldSpecificFieldKind::Count,
                Argument::Variable(var_ref),
            ) => {
                let variable_value =
                    usize_from_field_value(&query_arguments[&var_ref.variable_name])
                        .expect("for field value to be coercible to usize");
                Some(variable_value)
            }
            Operation::LessThan(FoldSpecificFieldKind::Count, Argument::Variable(var_ref)) => {
                let variable_value =
                    usize_from_field_value(&query_arguments[&var_ref.variable_name])
                        .expect("for field value to be coercible to usize");
                // saturating_sub() here is a safeguard against underflow: in principle,
                // we shouldn't see a comparison for "< 0", but if we do regardless, we'd prefer to
                // saturate to 0 rather than wrapping around. This check is an optimization and
                // is allowed to be more conservative than strictly necessary.
                // The later full application of filters ensures correctness.
                Some(variable_value.saturating_sub(1))
            }
            Operation::OneOf(FoldSpecificFieldKind::Count, Argument::Variable(var_ref)) => {
                match &query_arguments[&var_ref.variable_name] {
                    FieldValue::List(v) => v
                        .iter()
                        .map(|x| {
                            usize_from_field_value(x)
                                .expect("for field value to be coercible to usize")
                        })
                        .max(),
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

/// If this IRFold has a filter on the folded element count, and that filter imposes
/// a min size that can be statically determined, return that min size so it can
/// be used for further optimizations. Otherwise, return None.
fn get_min_fold_count_limit(carrier: &mut QueryCarrier, fold: &IRFold) -> Option<usize> {
    let mut result: Option<usize> = None;

    let query_arguments = &carrier.query.as_ref().expect("query was not returned").arguments;
    for post_fold_filter in fold.post_filters.iter() {
        let next_limit = match post_fold_filter {
            // We do not need to visit Equals and OneOf here,
            // since those will be handled by `get_max_fold_count_limit`
            Operation::GreaterThanOrEqual(
                FoldSpecificFieldKind::Count,
                Argument::Variable(var_ref),
            ) => {
                let variable_value =
                    usize_from_field_value(&query_arguments[&var_ref.variable_name])
                        .expect("for field value to be coercible to usize");
                Some(variable_value)
            }
            Operation::GreaterThan(FoldSpecificFieldKind::Count, Argument::Variable(var_ref)) => {
                let variable_value =
                    usize_from_field_value(&query_arguments[&var_ref.variable_name])
                        .expect("for field value to be coercible to usize");
                Some(variable_value + 1)
            }
            _ => None,
        };

        match (result, next_limit) {
            (None, _) => result = next_limit,
            (Some(l), Some(r)) if l < r => result = next_limit,
            _ => {}
        }
    }

    result
}

fn collect_fold_elements<'query, Vertex: Clone + Debug + 'query>(
    mut iterator: ContextIterator<'query, Vertex>,
    max_fold_count_limit: &Option<usize>,
    min_fold_count_limit: &Option<usize>,
    no_outputs_in_fold: bool,
    has_output_on_fold_count: bool,
) -> Option<Vec<DataContext<Vertex>>> {
    // We do not apply the min_fold_count_limit optimization if we have an upper bound,
    // in the form of max_fold_count_limit, because if we have a required upperbound,
    // then we can't stop at the lower bound, if we are required to observe whether we hit the
    // upperbound, it's no longer safe to stop at the lower bound.
    if let Some(max_fold_count_limit) = max_fold_count_limit {
        // If this fold has more than `max_fold_count_limit` elements,
        // it will get filtered out by a post-fold filter.
        // Pulling elements from `iterator` causes computations and data fetches to happen,
        // and as an optimization we'd like to stop pulling elements as soon as possible.
        // If we are able to pull more than `max_fold_count_limit + 1` elements,
        // we know that this fold is going to get filtered out, so we might as well
        // stop materializing its elements early. Limit the max allocation size since
        // it might not always be fully used.
        let mut fold_elements = Vec::with_capacity((*max_fold_count_limit).min(16));

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
        let collected = match min_fold_count_limit {
            // In this optimization we do not need to collect the entire fold if we can decide
            // that the user never expects to observe the length in the entire fold, therefore
            // performing less work.
            //
            // This optimization requires that the user can never observe that we didn't fully
            // compute the entire `@fold`. This is only possible if the fold has no outputs,
            // the user does not output the count of elements in the fold, and the user does
            // not use any of the filters that require knowing the exact count of elements
            // in the fold.
            //
            // When we do `iterator.take(*min_fold_count_limit)`, this statement takes at most
            // `min_fold_count_limit` elements, however if we have less the iterator can stop
            // early.
            Some(min_fold_count_limit) if no_outputs_in_fold && !has_output_on_fold_count => {
                iterator.take(*min_fold_count_limit).collect()
            }
            // We weren't able to find any early-termination condition for materializing the fold,
            // so materialize the whole thing and return it.
            _ => iterator.collect(),
        };

        Some(collected)
    }
}

#[allow(unused_variables)]
fn compute_fold<'query, AdapterT: Adapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    expanding_from: &IRVertex,
    parent_component: &IRQueryComponent,
    fold: Arc<IRFold>,
    mut iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    // Get any imported tag values needed inside the fold component or one of its subcomponents.
    for imported_field in fold.imported_tags.iter() {
        match &imported_field {
            FieldRef::ContextField(field) => {
                let vertex_id = field.vertex_id;
                let activated_vertex_iterator: ContextIterator<'query, AdapterT::Vertex> =
                    Box::new(iterator.map(move |x| x.activate_vertex(&vertex_id)));

                let field_vertex = &parent_component.vertices[&field.vertex_id];
                let type_name = &field_vertex.type_name;

                let query = carrier.query.take().expect("query was not returned");
                let resolve_info = ResolveInfo::new(query, vertex_id, true);

                let context_and_value_iterator = adapter.resolve_property(
                    activated_vertex_iterator,
                    type_name,
                    &field.field_name,
                    &resolve_info,
                );
                carrier.query = Some(resolve_info.into_inner());

                let cloned_field = imported_field.clone();
                iterator = Box::new(context_and_value_iterator.map(move |(mut context, value)| {
                    // Check whether the tagged value is coming from an `@optional` scope
                    // that did not exist, in order to satisfy its filtering semantics.
                    let tag_value = if context.vertices[&vertex_id].is_some() {
                        TaggedValue::Some(value)
                    } else {
                        TaggedValue::NonexistentOptional
                    };
                    context.imported_tags.insert(cloned_field.clone(), tag_value);
                    context
                }));
            }
            FieldRef::FoldSpecificField(fold_specific_field) => {
                let cloned_field = imported_field.clone();
                let fold_eid = fold_specific_field.fold_eid;
                iterator = Box::new(
                    compute_fold_specific_field_with_separate_value(
                        fold_specific_field.fold_eid,
                        &fold_specific_field.kind,
                        iterator,
                    )
                    .map(move |(mut ctx, tagged_value)| {
                        ctx.imported_tags.insert(cloned_field.clone(), tagged_value);
                        ctx
                    }),
                );
            }
        }
    }

    // Get the initial vertices inside the folded scope.
    let expanding_from_vid = expanding_from.vid;
    let activated_vertex_iterator: ContextIterator<'query, AdapterT::Vertex> =
        Box::new(iterator.map(move |x| x.activate_vertex(&expanding_from_vid)));
    let type_name = &expanding_from.type_name;

    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveEdgeInfo::new(query, expanding_from_vid, fold.to_vid, fold.eid);

    let edge_iterator = adapter.resolve_neighbors(
        activated_vertex_iterator,
        type_name,
        &fold.edge_name,
        &fold.parameters,
        &resolve_info,
    );
    carrier.query = Some(resolve_info.into_inner());

    // Materialize the full fold data.
    // These values are moved into the closure.
    let cloned_adapter = adapter.clone();
    let mut cloned_carrier = carrier.clone();
    let fold_component = fold.component.clone();
    let fold_eid = fold.eid;
    let max_fold_size = get_max_fold_count_limit(carrier, fold.as_ref());
    let min_fold_size = get_min_fold_count_limit(carrier, fold.as_ref());
    let no_outputs_in_fold = fold.component.outputs.is_empty();
    let has_output_on_fold_count =
        fold.fold_specific_outputs.values().any(|x| *x == FoldSpecificFieldKind::Count);
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
            &mut cloned_carrier,
            &fold_component,
            neighbor_contexts,
        );

        // Check whether this @fold is inside an @optional that doesn't exist.
        // This is not the same as having *zero* elements: nonexistent != empty.
        let fold_exists = context.vertices[&expanding_from_vid].is_some();
        let fold_elements = if fold_exists {
            // N.B.: Note the `?` at the end here!
            //       This lets us early-discard folds that failed a post-processing filter.
            Some(collect_fold_elements(
                computed_iterator,
                &max_fold_size,
                &min_fold_size,
                no_outputs_in_fold,
                has_output_on_fold_count,
            )?)
        } else {
            None
        };

        context.folded_contexts.insert_or_error(fold_eid, fold_elements).unwrap();

        // Remove no-longer-needed imported tags.
        for imported_tag in &moved_fold.imported_tags {
            context.imported_tags.remove(imported_tag).unwrap();
        }

        Some(context)
    });

    // Apply post-fold filters.
    let mut post_filtered_iterator: ContextIterator<'query, AdapterT::Vertex> =
        Box::new(folded_iterator);
    for post_fold_filter in fold.post_filters.iter() {
        post_filtered_iterator = apply_fold_specific_filter(
            adapter.as_ref(),
            carrier,
            parent_component,
            fold.as_ref(),
            expanding_from.vid,
            post_fold_filter,
            post_filtered_iterator,
        );
    }

    // Compute the outputs from this fold.
    let mut output_names: Vec<Arc<str>> = fold.component.outputs.keys().cloned().collect();
    output_names.sort_unstable(); // to ensure deterministic resolve_property() ordering

    let cloned_adapter = adapter.clone();
    let mut cloned_carrier = carrier.clone();
    let fold_component = fold.component.clone();
    let final_iterator = post_filtered_iterator.map(move |mut ctx| {
        let fold_elements = &ctx.folded_contexts[&fold_eid];
        debug_assert_eq!(
            // Two ways to check if the `@fold` is inside an `@optional` that didn't exist:
            ctx.vertices[&expanding_from_vid].is_some(),
            fold_elements.is_some(),
            "\
mismatch on whether the fold below {expanding_from_vid:?} was inside an `@optional`: {ctx:?}",
        );

        // Add any fold-specific field outputs to the context's folded values.
        for (output_name, fold_specific_field) in &fold.fold_specific_outputs {
            // If the @fold is inside an @optional that doesn't exist,
            // its outputs should be `null` rather than empty lists (the usual for empty folds).
            // Transformed outputs should also be `null` rather than their usual transformed defaults.
            let value = fold_elements.as_ref().map(|elements| match fold_specific_field {
                FoldSpecificFieldKind::Count => {
                    ValueOrVec::Value(FieldValue::Uint64(elements.len() as u64))
                }
            });
            ctx.folded_values
                .insert_or_error((fold_eid, output_name.clone()), value)
                .expect("this fold output was already computed");
        }

        // Prepare empty vectors for all the outputs from this @fold component.
        // If the fold-root vertex didn't exist, the default is `null` instead.
        let default_value =
            if fold_elements.is_some() { Some(ValueOrVec::Vec(vec![])) } else { None };
        let mut folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>> = output_names
            .iter()
            .map(|output| ((fold_eid, output.clone()), default_value.clone()))
            .collect();

        // Don't bother trying to resolve property values on this @fold when it's empty.
        // Skip the adapter resolve_property() calls and add the empty output values directly.
        let fold_contains_elements = fold_elements.as_ref().map(|e| !e.is_empty()).unwrap_or(false);
        if !fold_contains_elements {
            // We need to make sure any outputs from any nested @fold components (recursively)
            // are set to the default value (empty list if the @fold existed and was empty,
            // null if it didn't exist because it was inside an @optional).
            let mut queue: Vec<_> = fold_component.folds.values().collect();
            while let Some(inner_fold) = queue.pop() {
                for output in inner_fold.fold_specific_outputs.keys() {
                    folded_values.insert((inner_fold.eid, output.clone()), default_value.clone());
                }
                for output in inner_fold.component.outputs.keys() {
                    folded_values.insert((inner_fold.eid, output.clone()), default_value.clone());
                }
                queue.extend(inner_fold.component.folds.values());
            }
        } else {
            // Iterate through the elements of the fold and get the values we need.
            let mut output_iterator: ContextIterator<'query, AdapterT::Vertex> = Box::new(
                fold_elements.as_ref().expect("fold did not contain elements").clone().into_iter(),
            );
            for output_name in output_names.iter() {
                // This is a slimmed-down version of computing a context field:
                // - it does not restore the prior active vertex after getting each value
                // - it already knows that the context field is guaranteed to exist
                let context_field = &fold.component.outputs[output_name.as_ref()];
                let vertex_id = context_field.vertex_id;
                let moved_iterator = Box::new(output_iterator.map(move |context| {
                    let new_vertex = context.vertices[&vertex_id].clone();
                    context.move_to_vertex(new_vertex)
                }));

                let query = cloned_carrier.query.take().expect("query was not returned");
                let resolve_info = ResolveInfo::new(query, vertex_id, true);
                let field_data_iterator = cloned_adapter.resolve_property(
                    moved_iterator,
                    &fold.component.vertices[&vertex_id].type_name,
                    &context_field.field_name,
                    &resolve_info,
                );
                cloned_carrier.query = Some(resolve_info.into_inner());

                output_iterator = Box::new(field_data_iterator.map(|(mut context, value)| {
                    context.values.push(value);
                    context
                }));
            }

            for mut folded_context in output_iterator {
                for (key, value) in folded_context.folded_values {
                    folded_values
                        .entry(key)
                        .or_insert_with(|| Some(ValueOrVec::Vec(vec![])))
                        .as_mut()
                        .expect("not Some")
                        .as_mut_vec()
                        .expect("not a Vec")
                        .push(value.unwrap_or(ValueOrVec::Value(FieldValue::Null)));
                }

                // We pushed values onto folded_context.values with output names in increasing order
                // and we are now popping from the back. That means we're getting the highest name
                // first, so we should reverse our output_names iteration order.
                for output in output_names.iter().rev() {
                    let value = folded_context.values.pop().unwrap();
                    folded_values
                        .get_mut(&(fold_eid, output.clone()))
                        .expect("key not present")
                        .as_mut()
                        .expect("value was None")
                        .as_mut_vec()
                        .expect("not a Vec")
                        .push(ValueOrVec::Value(value));
                }
            }
        };

        let prior_folded_values_count = ctx.folded_values.len();
        let new_folded_values_count = folded_values.len();
        ctx.folded_values.extend(folded_values);

        // Ensure the merged maps had disjoint keys.
        assert_eq!(ctx.folded_values.len(), prior_folded_values_count + new_folded_values_count);

        ctx
    });

    Box::new(final_iterator)
}

fn apply_local_field_filter<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    filter: &Operation<LocalField, Argument>,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let local_field = filter.left();
    let field_iterator =
        compute_local_field(adapter, carrier, component, current_vid, local_field, iterator);

    apply_filter(
        adapter,
        carrier,
        component,
        current_vid,
        &filter.map(|_| (), |r| r),
        field_iterator,
    )
}

fn apply_fold_specific_filter<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    fold: &IRFold,
    current_vid: Vid,
    filter: &Operation<FoldSpecificFieldKind, Argument>,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let fold_specific_field = filter.left();
    let field_iterator = Box::new(compute_fold_specific_field_with_separate_value(fold.eid, fold_specific_field, iterator).map(|(mut ctx, tagged_value)| {
        let value = match tagged_value {
            TaggedValue::Some(value) => value,
            TaggedValue::NonexistentOptional => {
                unreachable!("while applying fold-specific filter, the @fold turned out to not exist: {ctx:?}")
            }
        };
        ctx.values.push(value);
        ctx
    }));

    apply_filter(
        adapter,
        carrier,
        component,
        current_vid,
        &filter.map(|_| (), |r| r),
        field_iterator,
    )
}

pub(super) fn compute_context_field_with_separate_value<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    context_field: &ContextField,
    iterator: Box<dyn Iterator<Item = DataContext<AdapterT::Vertex>> + 'query>,
) -> Box<dyn Iterator<Item = (DataContext<AdapterT::Vertex>, TaggedValue)> + 'query> {
    let vertex_id = context_field.vertex_id;

    if let Some(vertex) = component.vertices.get(&vertex_id) {
        let moved_iterator = iterator.map(move |mut context| {
            let active_vertex = context.active_vertex.clone();
            let new_vertex = context.vertices[&vertex_id].clone();
            context.suspended_vertices.push(active_vertex);
            context.move_to_vertex(new_vertex)
        });

        let type_name = &vertex.type_name;
        let query = carrier.query.take().expect("query was not returned");
        let resolve_info = ResolveInfo::new(query, vertex_id, true);

        let context_and_value_iterator = adapter
            .resolve_property(
                Box::new(moved_iterator),
                type_name,
                &context_field.field_name,
                &resolve_info,
            )
            .map(move |(mut context, value)| {
                let tagged_value = if context.vertices[&vertex_id].is_some() {
                    TaggedValue::Some(value)
                } else {
                    // The value is coming from an @optional scope that didn't exist.
                    TaggedValue::NonexistentOptional
                };

                // Make sure that the context has the same "current" token
                // as before evaluating the context field.
                let old_current_token = context.suspended_vertices.pop().unwrap();
                (context.move_to_vertex(old_current_token), tagged_value)
            });
        carrier.query = Some(resolve_info.into_inner());

        Box::new(context_and_value_iterator)
    } else {
        // This context field represents an imported tag value from an outer component.
        // Grab its value from the context itself.
        let field_ref = FieldRef::ContextField(context_field.clone());
        Box::new(iterator.map(move |context| {
            let value = context.imported_tags[&field_ref].clone();
            (context, value)
        }))
    }
}

pub(super) fn compute_fold_specific_field_with_separate_value<
    'query,
    Vertex: Clone + Debug + 'query,
>(
    fold_eid: Eid,
    fold_specific_field: &FoldSpecificFieldKind,
    iterator: ContextIterator<'query, Vertex>,
) -> ContextOutcomeIterator<'query, Vertex, TaggedValue> {
    match fold_specific_field {
        FoldSpecificFieldKind::Count => Box::new(iterator.map(move |ctx| {
            // TODO: Ensure output type inference handles this correctly too...
            let folded_contexts = ctx.folded_contexts[&fold_eid].as_ref();
            let value = match folded_contexts {
                None => TaggedValue::NonexistentOptional,
                Some(v) => TaggedValue::Some(FieldValue::Uint64(v.len() as u64)),
            };
            (ctx, value)
        })),
    }
}

pub(super) fn compute_local_field_with_separate_value<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    local_field: &LocalField,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextOutcomeIterator<'query, AdapterT::Vertex, FieldValue> {
    let type_name = &component.vertices[&current_vid].type_name;
    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveInfo::new(query, current_vid, true);

    let context_and_value_iterator =
        adapter.resolve_property(iterator, type_name, &local_field.field_name, &resolve_info);
    carrier.query = Some(resolve_info.into_inner());

    context_and_value_iterator
}

fn compute_local_field<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    local_field: &LocalField,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let context_and_value_iterator = compute_local_field_with_separate_value(
        adapter,
        carrier,
        component,
        current_vid,
        local_field,
        iterator,
    );

    Box::new(context_and_value_iterator.map(|(mut context, value)| {
        context.values.push(value);
        context
    }))
}

struct EdgeExpander<'query, Vertex: Clone + Debug + 'query> {
    context: DataContext<Vertex>,
    neighbors: VertexIterator<'query, Vertex>,
    is_optional_edge: bool,
    has_neighbors: bool,
    neighbors_ended: bool,
    ended: bool,
}

impl<'query, Vertex: Clone + Debug + 'query> EdgeExpander<'query, Vertex> {
    pub fn new(
        context: DataContext<Vertex>,
        neighbors: VertexIterator<'query, Vertex>,
        is_optional_edge: bool,
    ) -> EdgeExpander<'query, Vertex> {
        EdgeExpander {
            context,
            neighbors,
            is_optional_edge,
            has_neighbors: false,
            neighbors_ended: false,
            ended: false,
        }
    }
}

impl<'query, Vertex: Clone + Debug + 'query> Iterator for EdgeExpander<'query, Vertex> {
    type Item = DataContext<Vertex>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }

        if !self.neighbors_ended {
            let neighbor = self.neighbors.next();
            if neighbor.is_some() {
                self.has_neighbors = true;
                return Some(self.context.split_and_move_to_vertex(neighbor));
            } else {
                self.neighbors_ended = true;
            }
        }

        assert!(self.neighbors_ended);
        self.ended = true;

        // If there's no current vertex, there couldn't possibly be neighbors.
        // If this assertion trips, the adapter's resolve_neighbors() implementation illegally
        // returned neighbors for a non-existent vertex.
        if self.context.active_vertex.is_none() {
            assert!(!self.has_neighbors);
        }

        // If the current vertex is None, that means that a prior edge was optional and missing.
        // In that case, we couldn't possibly have found any neighbors here, but the optional-ness
        // of that prior edge means we have to return a context with no active vertex.
        //
        // The other case where we have to return a context with no active vertex is when
        // we have a current vertex, but the edge we're traversing is optional and does not exist.
        if self.context.active_vertex.is_none() || (!self.has_neighbors && self.is_optional_edge) {
            Some(self.context.split_and_move_to_vertex(None))
        } else {
            None
        }
    }
}

fn expand_edge<'query, AdapterT: Adapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    expanding_from_vid: Vid,
    expanding_to_vid: Vid,
    edge: &IREdge,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let expanded_iterator = if let Some(recursive) = &edge.recursive {
        expand_recursive_edge(
            adapter,
            carrier,
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
            adapter,
            carrier,
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
        carrier,
        component,
        &component.vertices[&expanding_to_vid],
        expanded_iterator,
    )
}

#[allow(clippy::too_many_arguments)]
fn expand_non_recursive_edge<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    _component: &IRQueryComponent,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &EdgeParameters,
    is_optional: bool,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let expanding_from_vid = expanding_from.vid;
    let expanding_vertex_iterator: ContextIterator<'query, AdapterT::Vertex> =
        Box::new(iterator.map(move |x| x.activate_vertex(&expanding_from_vid)));

    let type_name = &expanding_from.type_name;
    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveEdgeInfo::new(query, expanding_from_vid, expanding_to.vid, edge_id);

    let edge_iterator = adapter.resolve_neighbors(
        expanding_vertex_iterator,
        type_name,
        edge_name,
        edge_parameters,
        &resolve_info,
    );
    carrier.query = Some(resolve_info.into_inner());

    Box::new(edge_iterator.flat_map(move |(context, neighbor_iterator)| {
        EdgeExpander::new(context, neighbor_iterator, is_optional)
    }))
}

/// Apply all the operations needed at entry into a new vertex:
/// - coerce the type, if needed
/// - apply all local filters
/// - record the vertex at this Vid in the context
fn perform_entry_into_new_vertex<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    vertex: &IRVertex,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let vertex_id = vertex.vid;
    let mut iterator = coerce_if_needed(adapter, carrier, vertex, iterator);
    for filter_expr in vertex.filters.iter() {
        iterator =
            apply_local_field_filter(adapter, carrier, component, vertex_id, filter_expr, iterator);
    }
    Box::new(iterator.map(move |mut x| {
        x.record_vertex(vertex_id);
        x
    }))
}

#[allow(clippy::too_many_arguments)]
fn expand_recursive_edge<'query, AdapterT: Adapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &EdgeParameters,
    recursive: &Recursive,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let expanding_from_vid = expanding_from.vid;
    let mut recursion_iterator: ContextIterator<'query, AdapterT::Vertex> =
        Box::new(iterator.map(move |mut context| {
            if context.active_vertex.is_none() {
                // Mark that this vertex starts off with a None active_vertex value,
                // so the later unsuspend() call should restore it to such a state later.
                context.suspended_vertices.push(None);
            }
            context.activate_vertex(&expanding_from_vid)
        }));

    let max_depth = usize::from(recursive.depth);
    recursion_iterator = perform_one_recursive_edge_expansion(
        adapter,
        carrier,
        component,
        &expanding_from.type_name,
        expanding_from,
        expanding_to,
        edge_id,
        edge_name,
        edge_parameters,
        recursion_iterator,
    );

    let edge_endpoint_type =
        expanding_to.coerced_from_type.as_ref().unwrap_or(&expanding_to.type_name);
    let recursing_from = recursive.coerce_to.as_ref().unwrap_or(edge_endpoint_type);

    for _ in 2..=max_depth {
        if let Some(coerce_to) = recursive.coerce_to.as_ref() {
            let query = carrier.query.take().expect("query was not returned");
            let resolve_info = ResolveInfo::new(query, expanding_from_vid, false);

            let coercion_iter = adapter.resolve_coercion(
                recursion_iterator,
                edge_endpoint_type,
                coerce_to,
                &resolve_info,
            );
            carrier.query = Some(resolve_info.into_inner());

            // This coercion is unusual since it doesn't discard elements that can't be coerced.
            // This is because we still want to produce those elements, and we simply want to
            // not continue recursing deeper through them since they don't have the edge we need.
            recursion_iterator =
                Box::new(coercion_iter.map(
                    |(ctx, can_coerce)| {
                        if can_coerce {
                            ctx
                        } else {
                            ctx.ensure_suspended()
                        }
                    },
                ));
        }

        recursion_iterator = perform_one_recursive_edge_expansion(
            adapter,
            carrier,
            component,
            recursing_from,
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
fn perform_one_recursive_edge_expansion<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    _component: &IRQueryComponent,
    expanding_from_type: &Arc<str>,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &EdgeParameters,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveEdgeInfo::new(query, expanding_from.vid, expanding_to.vid, edge_id);

    let edge_iterator = adapter.resolve_neighbors(
        iterator,
        expanding_from_type,
        edge_name,
        edge_parameters,
        &resolve_info,
    );
    carrier.query = Some(resolve_info.into_inner());

    let result_iterator: ContextIterator<'query, AdapterT::Vertex> =
        Box::new(edge_iterator.flat_map(move |(context, neighbor_iterator)| {
            RecursiveEdgeExpander::new(context, neighbor_iterator)
        }));

    result_iterator
}

struct RecursiveEdgeExpander<'query, Vertex: Clone + Debug + 'query> {
    context: Option<DataContext<Vertex>>,
    neighbor_base: Option<DataContext<Vertex>>,
    neighbors: VertexIterator<'query, Vertex>,
    has_neighbors: bool,
    neighbors_ended: bool,
}

impl<'query, Vertex: Clone + Debug + 'query> RecursiveEdgeExpander<'query, Vertex> {
    pub fn new(
        context: DataContext<Vertex>,
        neighbors: VertexIterator<'query, Vertex>,
    ) -> RecursiveEdgeExpander<'query, Vertex> {
        RecursiveEdgeExpander {
            context: Some(context),
            neighbor_base: None,
            neighbors,
            has_neighbors: false,
            neighbors_ended: false,
        }
    }
}

impl<'query, Vertex: Clone + Debug + 'query> Iterator for RecursiveEdgeExpander<'query, Vertex> {
    type Item = DataContext<Vertex>;

    fn next(&mut self) -> Option<Self::Item> {
        if !self.neighbors_ended {
            let neighbor = self.neighbors.next();

            if let Some(vertex) = neighbor {
                if let Some(context) = self.context.take() {
                    // Prep a neighbor base context for future use, since we're moving
                    // the "self" context out.
                    self.neighbor_base = Some(context.split_and_move_to_vertex(None));

                    // Attach the "self" context as a piggyback rider on the neighbor.
                    let mut neighbor_context = context.split_and_move_to_vertex(Some(vertex));
                    neighbor_context
                        .piggyback
                        .get_or_insert_with(Default::default)
                        .push(context.ensure_suspended());
                    return Some(neighbor_context);
                } else {
                    // The "self" vertex has already been moved out, so use the neighbor base context
                    // as the starting point for constructing a new context.
                    return Some(
                        self.neighbor_base.as_ref().unwrap().split_and_move_to_vertex(Some(vertex)),
                    );
                }
            } else {
                self.neighbors_ended = true;

                // If there's no current vertex, there couldn't possibly be neighbors.
                // If this assertion trips, the adapter's resolve_neighbors() implementation
                // illegally returned neighbors for a non-existent vertex.
                if let Some(context) = &self.context {
                    if context.active_vertex.is_none() {
                        assert!(!self.has_neighbors);
                    }
                }
            }
        }

        self.context.take()
    }
}

fn unpack_piggyback<Vertex: Debug + Clone>(
    context: DataContext<Vertex>,
) -> Vec<DataContext<Vertex>> {
    let mut result = Default::default();

    unpack_piggyback_inner(&mut result, context);

    result
}

fn unpack_piggyback_inner<Vertex: Debug + Clone>(
    output: &mut Vec<DataContext<Vertex>>,
    mut context: DataContext<Vertex>,
) {
    if let Some(mut piggyback) = context.piggyback.take() {
        for ctx in piggyback.drain(..) {
            unpack_piggyback_inner(output, ctx);
        }
    }

    output.push(context);
}

fn post_process_recursive_expansion<'query, Vertex: Clone + Debug + 'query>(
    iterator: ContextIterator<'query, Vertex>,
) -> ContextIterator<'query, Vertex> {
    Box::new(iterator.flat_map(|context| unpack_piggyback(context)).map(|context| {
        assert!(context.piggyback.is_none());
        context.ensure_unsuspended()
    }))
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
        ir::{FieldValue, IndexedQuery},
        test_types::{TestIRQueryResult, TestInterpreterOutputData},
    };

    #[parameterize("trustfall_core/test_data/tests/valid_queries")]
    fn parameterized_output_metadata_tester(base: &Path, stem: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{stem}.ir.ron"));

        let input_data = fs::read_to_string(input_path).unwrap();
        let test_query: TestIRQueryResult = ron::from_str(&input_data).unwrap();
        let test_query = test_query.unwrap();

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{stem}.output.ron"));
        let check_data = fs::read_to_string(check_path).unwrap();
        let expected_output_data: TestInterpreterOutputData = ron::from_str(&check_data).unwrap();

        let indexed_query: IndexedQuery =
            test_query.ir_query.try_into().expect("failed to create IndexedQuery");
        assert_eq!(expected_output_data.outputs, indexed_query.outputs);
    }

    #[parameterize("trustfall_core/test_data/tests/execution_errors")]
    fn parameterized_execution_error_tester(base: &Path, stem: &str) {
        let mut input_path = PathBuf::from(base);
        input_path.push(format!("{stem}.ir.ron"));

        let mut check_path = PathBuf::from(base);
        check_path.push(format!("{stem}.exec-error.ron"));
        let check_data = fs::read_to_string(check_path).unwrap();

        let input_data = fs::read_to_string(input_path).unwrap();
        let test_query: TestIRQueryResult = ron::from_str(&input_data).unwrap();
        let test_query = test_query.unwrap();

        let arguments: BTreeMap<Arc<str>, FieldValue> =
            test_query.arguments.into_iter().map(|(k, v)| (Arc::from(k), v)).collect();

        let indexed_query: IndexedQuery = test_query.ir_query.try_into().unwrap();
        let constructed_test_item = InterpretedQuery::from_query_and_arguments(
            Arc::from(indexed_query),
            Arc::from(arguments),
        );

        let check_parsed: Result<_, QueryArgumentsError> = Err(ron::from_str(&check_data).unwrap());

        assert_eq!(check_parsed, constructed_test_item);
    }

    mod batching_fuzzer_repro_cases {
        use std::{cell::RefCell, collections::VecDeque, marker::PhantomData, sync::Arc};

        use crate::{
            interpreter::{
                execution::interpret_ir, Adapter, ContextIterator, ContextOutcomeIterator,
                ResolveEdgeInfo, ResolveInfo, VertexIterator,
            },
            ir::{EdgeParameters, FieldValue, IndexedQuery},
            numbers_interpreter::NumbersAdapter,
            test_types::{TestIRQuery, TestInterpreterOutputData},
        };

        struct VariableChunkIterator<I: Iterator> {
            iter: I,
            buffer: VecDeque<I::Item>,
            chunk_sequence: u64,
            offset: usize,
        }

        impl<I: Iterator> VariableChunkIterator<I> {
            fn new(iter: I, chunk_sequence: u64) -> Self {
                let mut value =
                    Self { iter, buffer: VecDeque::with_capacity(4), chunk_sequence, offset: 0 };

                // Eagerly advancing the input iterator is important because that's how we repro:
                // https://github.com/obi1kenobi/trustfall/issues/205
                let chunk_size = value.next_chunk_size();
                value.buffer.extend(value.iter.by_ref().take(chunk_size));
                value
            }

            fn next_chunk_size(&mut self) -> usize {
                let next_chunk = ((self.chunk_sequence >> self.offset) & 3) + 1;
                if self.offset >= 62 {
                    self.offset = 0;
                } else {
                    self.offset += 2;
                }
                assert!(next_chunk >= 1);
                next_chunk as usize
            }
        }

        impl<I: Iterator> Iterator for VariableChunkIterator<I> {
            type Item = I::Item;

            fn next(&mut self) -> Option<Self::Item> {
                if let Some(element) = self.buffer.pop_front() {
                    Some(element)
                } else {
                    let next = self.iter.next();
                    if next.is_some() {
                        let elements_to_buffer = self.next_chunk_size() - 1;
                        self.buffer.extend(self.iter.by_ref().take(elements_to_buffer));
                    }
                    next
                }
            }
        }

        struct VariableBatchingAdapter<'a, AdapterT: Adapter<'a> + 'a> {
            adapter: AdapterT,
            batch_sequences: RefCell<VecDeque<u64>>,
            _marker: PhantomData<&'a ()>,
        }

        impl<'a, AdapterT: Adapter<'a> + 'a> VariableBatchingAdapter<'a, AdapterT> {
            fn new(adapter: AdapterT, batch_sequences: VecDeque<u64>) -> Self {
                Self {
                    adapter,
                    batch_sequences: RefCell::new(batch_sequences),
                    _marker: PhantomData,
                }
            }
        }

        impl<'a, AdapterT: Adapter<'a> + 'a> Adapter<'a> for VariableBatchingAdapter<'a, AdapterT> {
            type Vertex = AdapterT::Vertex;

            fn resolve_starting_vertices(
                &self,
                edge_name: &Arc<str>,
                parameters: &EdgeParameters,
                resolve_info: &ResolveInfo,
            ) -> VertexIterator<'a, Self::Vertex> {
                let mut batch_sequences_ref = self.batch_sequences.borrow_mut();
                let sequence = batch_sequences_ref.pop_front().unwrap_or(0);
                drop(batch_sequences_ref);

                let inner =
                    self.adapter.resolve_starting_vertices(edge_name, parameters, resolve_info);
                Box::new(VariableChunkIterator::new(inner, sequence))
            }

            fn resolve_property(
                &self,
                contexts: ContextIterator<'a, Self::Vertex>,
                type_name: &Arc<str>,
                property_name: &Arc<str>,
                resolve_info: &ResolveInfo,
            ) -> ContextOutcomeIterator<'a, Self::Vertex, FieldValue> {
                let mut batch_sequences_ref = self.batch_sequences.borrow_mut();
                let sequence = batch_sequences_ref.pop_front().unwrap_or(0);
                drop(batch_sequences_ref);

                let inner = self.adapter.resolve_property(
                    Box::new(contexts),
                    type_name,
                    property_name,
                    resolve_info,
                );
                Box::new(VariableChunkIterator::new(inner, sequence))
            }

            fn resolve_neighbors(
                &self,
                contexts: ContextIterator<'a, Self::Vertex>,
                type_name: &Arc<str>,
                edge_name: &Arc<str>,
                parameters: &EdgeParameters,
                resolve_info: &ResolveEdgeInfo,
            ) -> ContextOutcomeIterator<'a, Self::Vertex, VertexIterator<'a, Self::Vertex>>
            {
                let mut batch_sequences_ref = self.batch_sequences.borrow_mut();
                let sequence = batch_sequences_ref.pop_front().unwrap_or(0);
                drop(batch_sequences_ref);

                let inner = self.adapter.resolve_neighbors(
                    contexts,
                    type_name,
                    edge_name,
                    parameters,
                    resolve_info,
                );
                Box::new(VariableChunkIterator::new(inner, sequence))
            }

            fn resolve_coercion(
                &self,
                contexts: ContextIterator<'a, Self::Vertex>,
                type_name: &Arc<str>,
                coerce_to_type: &Arc<str>,
                resolve_info: &ResolveInfo,
            ) -> ContextOutcomeIterator<'a, Self::Vertex, bool> {
                let mut batch_sequences_ref = self.batch_sequences.borrow_mut();
                let sequence = batch_sequences_ref.pop_front().unwrap_or(0);
                drop(batch_sequences_ref);

                let inner = self.adapter.resolve_coercion(
                    contexts,
                    type_name,
                    coerce_to_type,
                    resolve_info,
                );
                Box::new(VariableChunkIterator::new(inner, sequence))
            }
        }

        fn run_test(file_stub: &str, batch_sequences: Vec<u64>) {
            let contents = std::fs::read_to_string(format!(
                "test_data/tests/valid_queries/{file_stub}.ir.ron"
            ))
            .expect("failed to read file");
            let input_data: TestIRQuery = ron::from_str::<Result<TestIRQuery, ()>>(&contents)
                .expect("failed to parse file")
                .expect("Err result");

            let output_contents = std::fs::read_to_string(format!(
                "test_data/tests/valid_queries/{file_stub}.output.ron"
            ))
            .expect("failed to read file");
            let output_data: TestInterpreterOutputData =
                ron::from_str(&output_contents).expect("failed to parse file");

            let batch_sequences: VecDeque<u64> = batch_sequences.into_iter().collect();

            let indexed_query: Arc<IndexedQuery> =
                Arc::new(input_data.ir_query.try_into().unwrap());
            assert_eq!(&output_data.outputs, &indexed_query.outputs);

            let arguments =
                Arc::new(input_data.arguments.into_iter().map(|(k, v)| (k.into(), v)).collect());
            let adapter =
                Arc::new(VariableBatchingAdapter::new(NumbersAdapter::new(), batch_sequences));
            let actual_results: Vec<_> =
                interpret_ir(adapter, indexed_query, arguments).unwrap().collect();

            assert_eq!(output_data.results, actual_results);
        }

        /// Reentrancy crash when `@output` resolution eagerly pulls
        /// multiple upstream contexts which subsequently need to enter and resolve a `@fold`:
        /// https://github.com/obi1kenobi/trustfall/issues/205
        #[test]
        fn repro_issue_205() {
            let input_file = "outputs_both_inside_and_outside_fold";
            let batch_sequences = vec![0, 0, u64::MAX];

            run_test(input_file, batch_sequences);
        }
    }
}
