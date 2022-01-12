use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt::Debug,
    num::NonZeroUsize,
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
        indexed::IndexedQuery, Argument, ContextField, EdgeParameters, Eid, FieldValue, IREdge,
        IRFold, IRQueryComponent, IRVertex, LocalField, Operation, Vid,
    },
};

use super::{error::QueryArgumentsError, Adapter, DataContext, InterpretedQuery};

#[allow(clippy::type_complexity)]
pub fn interpret_ir<'query, DataToken>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    indexed_query: Arc<IndexedQuery>,
    arguments: Arc<HashMap<Arc<str>, FieldValue>>,
) -> Result<Box<dyn Iterator<Item = HashMap<Arc<str>, FieldValue>> + 'query>, QueryArgumentsError>
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
        Some(coerced_from) => {
            let mut adapter_ref = adapter.borrow_mut();
            let coercion_iter = adapter_ref.can_coerce_to_type(
                iterator,
                coerced_from.clone(),
                vertex.type_name.clone(),
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
    }
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
        iterator = apply_filter(
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

    let mut visited_vids: HashSet<Vid> = hashset! {component_root_vid};

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
                fold,
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
) -> Box<dyn Iterator<Item = HashMap<Arc<str>, FieldValue>> + 'query> {
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

    Box::new(output_iterator.map(move |mut context| {
        assert!(context.values.len() == output_names.len());

        let mut output: HashMap<Arc<str>, FieldValue> = output_names
            .iter()
            .cloned()
            .zip(context.values.drain(..))
            .collect();

        for ((_, output_name), output_value) in context.folded_values.drain() {
            let existing = output.insert(output_name, output_value.into());
            assert!(existing.is_none());
        }

        output
    }))
}

#[allow(unused_variables)]
fn compute_fold<'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    expanding_from: &IRVertex,
    fold: &IRFold,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let expanding_from_vid = expanding_from.vid;
    let activated_vertex_iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
        Box::new(iterator.map(move |x| x.activate_token(&expanding_from_vid)));

    let current_type_name = &expanding_from.type_name;
    let mut adapter_ref = adapter.borrow_mut();
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

    let cloned_adapter = adapter.clone();
    let cloned_query = query.clone();
    let fold_component = fold.component.clone();
    let fold_eid = fold.eid;
    let folded_iterator = edge_iterator.map(move |(mut context, neighbors)| {
        let neighbor_contexts = Box::new(neighbors.map(|x| DataContext::new(Some(x))));

        let computed_iterator = compute_component(
            cloned_adapter.clone(),
            &cloned_query,
            &fold_component,
            neighbor_contexts,
        );

        // TODO: apply post-fold filters here

        let mut output_names: Vec<Arc<str>> = fold_component.outputs.keys().cloned().collect();
        output_names.sort_unstable(); // to ensure deterministic project_property() ordering

        let mut output_iterator = computed_iterator;
        for output_name in output_names.iter() {
            let context_field = &fold_component.outputs[output_name.as_ref()];
            let vertex_id = context_field.vertex_id;
            let moved_iterator = Box::new(output_iterator.map(move |context| {
                let new_token = context.tokens[&vertex_id].clone();
                context.move_to_token(new_token)
            }));

            let mut adapter_ref = cloned_adapter.borrow_mut();
            let field_data_iterator = adapter_ref.project_property(
                moved_iterator,
                fold_component.vertices[&vertex_id].type_name.clone(),
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

        let mut folded_values: HashMap<(Eid, Arc<str>), Vec<ValueOrVec>> = output_names
            .iter()
            .map(|output| ((fold_eid, output.clone()), Vec::new()))
            .collect();
        for mut folded_context in output_iterator {
            for (key, values) in folded_context.folded_values.drain() {
                folded_values
                    .entry(key)
                    .or_default()
                    .push(ValueOrVec::Vec(values));
            }

            // We pushed values onto folded_context.values with output names in increasing order
            // and we are now popping from the back. That means we're getting the highest name
            // first, so we should reverse our output_names iteration order.
            for output in output_names.iter().rev() {
                let value = folded_context.values.pop().unwrap();
                folded_values
                    .get_mut(&(fold_eid, output.clone()))
                    .unwrap()
                    .push(ValueOrVec::Value(value));
            }
        }

        context.folded_values = folded_values;

        context
    });

    Box::new(folded_iterator)
}

macro_rules! implement_filter {
    ( $iter: ident, $right: ident, $func: ident ) => {
        Box::new($iter.filter_map(move |mut context| {
            let right_value = context.values.pop().unwrap();
            let left_value = context.values.pop().unwrap();
            if let Argument::Tag(field) = &$right {
                let tag_optional_and_missing = context.tokens[&field.vertex_id].is_none();
                if tag_optional_and_missing {
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
                let tag_optional_and_missing = context.tokens[&field.vertex_id].is_none();
                if tag_optional_and_missing {
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

fn apply_filter<'query, DataToken: Clone + Debug + 'query>(
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

    let expression_iterator = match filter.right() {
        Some(Argument::Tag(context_field)) => {
            compute_context_field(adapter_ref, query, component, context_field, field_iterator)
        }
        Some(Argument::Variable(var)) => {
            let right_value = query.arguments[var.variable_name.as_ref()].to_owned();
            Box::new(field_iterator.map(move |mut ctx| {
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
        None => field_iterator,
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
    let moved_iterator = iterator.map(move |mut context| {
        let current_token = context.current_token.clone();
        let new_token = context.tokens[&vertex_id].clone();
        context.suspended_tokens.push(current_token);
        context.move_to_token(new_token)
    });

    let current_type_name = &component.vertices[&vertex_id].type_name;
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
    if let Some(depth) = edge.recursive {
        expand_recursive_edge(
            adapter,
            query,
            component,
            &component.vertices[&expanding_from_vid],
            &component.vertices[&expanding_to_vid],
            edge.eid,
            &edge.edge_name,
            &edge.parameters,
            depth,
            iterator,
        )
    } else {
        expand_non_recursive_edge(
            adapter,
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
    }
}

#[allow(clippy::too_many_arguments)]
fn expand_non_recursive_edge<'schema, 'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &Option<Arc<EdgeParameters>>,
    is_optional: bool,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let expanding_from_vid = expanding_from.vid;
    let expanding_to_vid = expanding_to.vid;
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

    let mut iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
        Box::new(edge_iterator.flat_map(move |(context, neighbor_iterator)| {
            EdgeExpander::new(context, neighbor_iterator, is_optional)
        }));

    iterator = coerce_if_needed(adapter.as_ref(), query, expanding_to, iterator);

    for filter_expr in expanding_to.filters.iter() {
        iterator = apply_filter(
            adapter.as_ref(),
            query,
            component,
            expanding_to_vid,
            filter_expr,
            iterator,
        );
    }

    Box::new(iterator.map(move |mut x| {
        x.record_token(expanding_to_vid);
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
    depth: NonZeroUsize,
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

    let max_depth = usize::from(depth);
    for _ in 1..=max_depth {
        recursion_iterator = perform_one_recursive_edge_expansion(
            adapter.clone(),
            query,
            component,
            expanding_from,
            expanding_to,
            edge_id,
            edge_name,
            edge_parameters,
            recursion_iterator,
        );
    }

    post_process_recursive_expansion(adapter, query, component, expanding_to, recursion_iterator)
}

#[allow(clippy::too_many_arguments)]
fn perform_one_recursive_edge_expansion<'schema, 'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    _component: &IRQueryComponent,
    expanding_from: &IRVertex,
    _expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &Option<Arc<EdgeParameters>>,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    // TODO: For situations where B is a subtype of A, and the recursive edge is defined as B->B,
    //       this current_type_name will continue to say the type is A at all depth levels,
    //       even though at all levels past the first, the type is actually B. Is this a problem?
    let current_type_name = &expanding_from.type_name;
    let mut adapter_ref = adapter.borrow_mut();
    let edge_iterator = adapter_ref.project_neighbors(
        iterator,
        current_type_name.clone(),
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

fn post_process_recursive_expansion<'schema, 'query, DataToken: Clone + Debug + 'query>(
    adapter: Rc<RefCell<impl Adapter<'query, DataToken = DataToken> + 'query>>,
    query: &InterpretedQuery,
    component: &IRQueryComponent,
    expanding_to: &IRVertex,
    iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query>,
) -> Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> {
    let expanding_to_vid = expanding_to.vid;
    let mut filtering_iterator: Box<dyn Iterator<Item = DataContext<DataToken>> + 'query> =
        Box::new(
            iterator
                .flat_map(|context| unpack_piggyback(context))
                .map(|context| {
                    assert!(context.piggyback.is_none());
                    context.ensure_unsuspended()
                }),
        );

    filtering_iterator =
        coerce_if_needed(adapter.as_ref(), query, expanding_to, filtering_iterator);

    for filter_expr in expanding_to.filters.iter() {
        filtering_iterator = apply_filter(
            adapter.as_ref(),
            query,
            component,
            expanding_to.vid,
            filter_expr,
            filtering_iterator,
        );
    }

    Box::new(filtering_iterator.map(move |mut x| {
        x.record_token(expanding_to_vid);
        x
    }))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use filetests_proc_macro::parameterize;

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

        let arguments: HashMap<Arc<str>, FieldValue> = test_query
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
