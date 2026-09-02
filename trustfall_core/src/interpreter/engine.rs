//! The shared stream execution kernel.
//!
//! Async adapters suspend while resolving data. The synchronous frontend supplies ready streams
//! and projects results back to an iterator. Internal streams carry errors; adapter resolvers
//! receive plain contexts. [`begin_stage`] and [`finish_stage`] bridge those two protocols.

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll},
};

use async_stream::try_stream;
use futures_core::Stream;
use futures_util::StreamExt;
use maplit::btreeset;

use crate::{
    ir::{
        Argument, EdgeParameters, Eid, FieldRef, FieldValue, FoldSpecificFieldKind, IREdge, IRFold,
        IRQueryComponent, IRVertex, IndexedQuery, LocalField, Operation, Vid,
    },
    util::BTreeMapTryInsertExt,
};

use super::{
    DataContext, InterpretedQuery, ResolveInfo, TaggedValue, ValueOrVec,
    async_adapter::{AsyncAdapter, ContextOutcomeStream, ContextStream},
    error::{ExecutionError, QueryArgumentsError},
    execution::{QueryCarrier, get_max_fold_count_limit, get_min_fold_count_limit},
    filtering::{ComparisonOp, ValuePredicate},
};

/// An internal context stream that ends after its first error.
pub type FallibleContextStream<'vertex, VertexT, E> =
    Pin<Box<dyn Stream<Item = Result<DataContext<VertexT>, E>> + 'vertex>>;

/// The endpoints and identity of an edge being expanded.
pub(super) struct EdgeRef<'a> {
    pub(super) from: &'a IRVertex,
    pub(super) to: &'a IRVertex,
    pub(super) eid: Eid,
    pub(super) name: &'a Arc<str>,
    pub(super) parameters: &'a EdgeParameters,
}

impl<'a> EdgeRef<'a> {
    fn new(from: &'a IRVertex, to: &'a IRVertex, edge: &'a IREdge) -> Self {
        Self { from, to, eid: edge.eid, name: &edge.edge_name, parameters: &edge.parameters }
    }
}

/// The upstream error path for one resolver stage.
#[must_use = "a resolver stage continuation must be passed to finish_stage"]
pub(super) struct StageContinuation<E> {
    pending_error: Rc<Cell<Option<E>>>,
}

impl<E> StageContinuation<E> {
    fn take_error(self) -> Option<E> {
        self.pending_error.take()
    }
}

/// Yields successful contexts and records the first upstream error.
struct TakeOk<'vertex, V, E> {
    input: FallibleContextStream<'vertex, V, E>,
    pending_error: Rc<Cell<Option<E>>>,
    done: bool,
}

impl<'vertex, V, E> Stream for TakeOk<'vertex, V, E> {
    type Item = DataContext<V>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        match this.input.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(ctx))) => Poll::Ready(Some(ctx)),
            Poll::Ready(Some(Err(error))) => {
                this.pending_error.set(Some(error));
                this.done = true;
                Poll::Ready(None)
            }
            Poll::Ready(None) => {
                this.done = true;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Reconnects a resolver's outcomes with the upstream error path.
struct FailFast<'vertex, V, O, E> {
    outcomes: ContextOutcomeStream<'vertex, V, Result<O, E>>,
    continuation: Option<StageContinuation<E>>,
    finished: bool,
}

impl<'vertex, V, O, E> Stream for FailFast<'vertex, V, O, E> {
    type Item = Result<(DataContext<V>, O), E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.finished {
            return Poll::Ready(None);
        }
        match this.outcomes.as_mut().poll_next(cx) {
            Poll::Ready(Some((ctx, Ok(value)))) => Poll::Ready(Some(Ok((ctx, value)))),
            Poll::Ready(Some((_, Err(error)))) => {
                this.finished = true;
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                this.finished = true;
                let upstream = this.continuation.take().and_then(StageContinuation::take_error);
                match upstream {
                    Some(error) => Poll::Ready(Some(Err(error))),
                    None => Poll::Ready(None),
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Split a fallible stream into adapter contexts and its upstream error path.
pub(super) fn begin_stage<'vertex, V, E>(
    input: FallibleContextStream<'vertex, V, E>,
) -> (ContextStream<'vertex, V>, StageContinuation<E>)
where
    V: Clone + Debug + 'vertex,
    E: 'vertex,
{
    let pending_error = Rc::new(Cell::new(None));
    let plain: ContextStream<'vertex, V> =
        Box::pin(TakeOk { input, pending_error: pending_error.clone(), done: false });
    (plain, StageContinuation { pending_error })
}

/// Merge resolver outcomes into a fail-fast engine stream.
#[allow(clippy::type_complexity)]
pub(super) fn finish_stage<'vertex, V, O, E>(
    outcomes: ContextOutcomeStream<'vertex, V, Result<O, E>>,
    continuation: StageContinuation<E>,
) -> Pin<Box<dyn Stream<Item = Result<(DataContext<V>, O), E>> + 'vertex>>
where
    V: 'vertex,
    O: 'vertex,
    E: 'vertex,
{
    Box::pin(FailFast { outcomes, continuation: Some(continuation), finished: false })
}

/// Execute an indexed query lazily as a fail-fast stream.
#[allow(clippy::type_complexity)]
pub fn interpret_ir<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    indexed_query: Arc<IndexedQuery>,
    arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
) -> Result<
    Pin<
        Box<
            dyn Stream<
                    Item = Result<BTreeMap<Arc<str>, FieldValue>, ExecutionError<AdapterT::Error>>,
                > + 'query,
        >,
    >,
    QueryArgumentsError,
> {
    let query = InterpretedQuery::from_query_and_arguments(indexed_query, arguments)?;
    let root_vid = query.indexed_query.ir_query.root_component.root;

    let ir_query = &query.indexed_query.ir_query;
    let root_edge = ir_query.root_name.clone();
    let root_edge_parameters = ir_query.root_parameters.clone();

    let mut carrier = QueryCarrier { query: None };
    let resolve_info = ResolveInfo::new(query.clone(), root_vid, false);

    // Eager, synchronous construction: call the adapter for starting vertices, wrap each into a
    // root context. Errors on starting vertices are threaded natively into the fallible stream.
    let starting =
        adapter.resolve_starting_vertices(&root_edge, &root_edge_parameters, &resolve_info);
    carrier.query = Some(resolve_info.into_inner());

    let mut stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
        Box::pin(starting.map(|result| result.map(|vertex| DataContext::new(Some(vertex)))));

    let component = ir_query.root_component.clone();
    stream = compute_component(adapter.clone(), &mut carrier, &component, stream);

    let outputs = construct_outputs(adapter.as_ref(), &mut carrier, stream);

    // Wrap the raw adapter error type into `ExecutionError` at the boundary, matching the sync API.
    Ok(Box::pin(outputs.map(|result| result.map_err(ExecutionError::Adapter))))
}

pub(super) fn compute_component<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    mut stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let component_root_vid = component.root;
    let root_vertex = &component.vertices[&component_root_vid];

    stream = coerce_if_needed(adapter.as_ref(), carrier, root_vertex, stream);

    for filter_expr in &root_vertex.filters {
        stream = apply_local_field_filter(
            adapter.as_ref(),
            carrier,
            component,
            component_root_vid,
            filter_expr,
            stream,
        );
    }

    stream = Box::pin(stream.map(move |result| {
        result.map(|mut context| {
            context.record_vertex(component_root_vid);
            context
        })
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

        assert!(process_next_fold.is_some() != process_next_edge.is_some());

        if let Some(fold) = process_next_fold {
            let from_vid_unvisited = visited_vids.insert(fold.from_vid);
            let to_vid_unvisited = visited_vids.insert(fold.to_vid);
            assert!(!from_vid_unvisited);
            assert!(to_vid_unvisited);

            stream = compute_fold(
                adapter.clone(),
                carrier,
                &component.vertices[&fold.from_vid],
                component,
                fold.clone(),
                stream,
            );

            next_fold = fold_iter.next();
        } else if let Some(edge) = process_next_edge {
            let from_vid_unvisited = visited_vids.insert(edge.from_vid);
            let to_vid_unvisited = visited_vids.insert(edge.to_vid);
            assert!(!from_vid_unvisited);
            assert!(to_vid_unvisited);

            stream = expand_edge(
                adapter.clone(),
                carrier,
                component,
                edge.from_vid,
                edge.to_vid,
                edge,
                stream,
            );

            next_edge = edge_iter.next();
        }
    }

    stream
}

/// Resolve a local `@filter` field and retain matching contexts.
fn apply_local_field_filter<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    filter: &Operation<LocalField, Argument>,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let local_field = filter.left();

    // Resolve the local field's value and push it onto each context's value stack.
    let type_name = component.vertices[&current_vid].type_name.clone();
    let (plain, upstream_error) = begin_stage(stream);
    let field_data = carrier.resolve_with(current_vid, true, |info| {
        adapter.resolve_property(plain, &type_name, &local_field.field_name, info)
    });

    let staged = finish_stage(field_data, upstream_error);
    let with_value: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
        Box::pin(staged.map(|result| {
            result.map(|(mut context, value)| {
                context.values.push(value);
                context
            })
        }));

    // Tag-argument filters must resolve the tag's value per context (an adapter call), so they're
    // handled by a dedicated stage rather than a pure predicate. The left field value has already
    // been pushed onto each context's value stack above.
    let filter_without_field = filter.map(|_| (), |r| r);
    if matches!(filter_without_field.right(), Some(Argument::Tag(_))) {
        return super::engine_filter::apply_tagged_filter(
            adapter,
            carrier,
            component,
            current_vid,
            filter,
            with_value,
        );
    }

    // Build the per-value predicate for unary / static-argument filters, matching the sync engine.
    let predicate = build_value_predicate(carrier, &filter_without_field);

    filter_by_predicate(with_value, predicate)
}

/// Construct a predicate for a unary or runtime-argument filter.
fn build_value_predicate(
    carrier: &QueryCarrier,
    filter_without_field: &Operation<(), &Argument>,
) -> ValuePredicate {
    if let Some(unary) = ValuePredicate::unary(filter_without_field) {
        return unary;
    }
    match filter_without_field.right() {
        Some(Argument::Variable(var)) => {
            let right_value = carrier.query.as_ref().expect("query was not returned").arguments
                [var.variable_name.as_ref()]
            .clone();
            ValuePredicate::static_argument(filter_without_field, right_value)
        }
        Some(Argument::Tag(_)) => unreachable!("tag filters handled by the tag-filter stage"),
        None => unreachable!("non-unary filter with no argument: {filter_without_field:?}"),
    }
}

fn coerce_if_needed<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    vertex: &IRVertex,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    match vertex.coerced_from_type.as_ref() {
        None => stream,
        Some(coerced_from) => {
            perform_coercion(adapter, carrier, vertex, coerced_from, &vertex.type_name, stream)
        }
    }
}

fn perform_coercion<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    vertex: &IRVertex,
    coerced_from: &Arc<str>,
    coerce_to: &Arc<str>,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let (plain, upstream_error) = begin_stage(stream);

    let coercion_outcomes = carrier.resolve_with(vertex.vid, false, |info| {
        adapter.resolve_coercion(plain, coerced_from, coerce_to, info)
    });

    let staged = finish_stage(coercion_outcomes, upstream_error);
    Box::pin(try_stream! {
        let mut staged = staged;
        while let Some(item) = staged.next().await {
            let (ctx, can_coerce) = item?;
            // Keep the vertex if the coercion succeeded, or if there's no vertex to coerce because
            // we're inside an `@optional` that didn't exist (coercion result is then irrelevant).
            if can_coerce || ctx.active_vertex.is_none() {
                yield ctx;
            }
        }
    })
}

fn expand_edge<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    expanding_from_vid: Vid,
    expanding_to_vid: Vid,
    edge: &IREdge,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let edge_ref = EdgeRef::new(
        &component.vertices[&expanding_from_vid],
        &component.vertices[&expanding_to_vid],
        edge,
    );

    let expanded = if let Some(recursive) = &edge.recursive {
        super::engine_recurse::expand_recursive_edge(
            adapter.clone(),
            carrier,
            &edge_ref,
            recursive,
            stream,
        )
    } else {
        expand_non_recursive_edge(adapter.as_ref(), carrier, &edge_ref, edge.optional, stream)
    };

    // Recurse into the neighboring vertex's own component processing (coercions, filters,
    // sub-edges), exactly as the sync engine does via `expand_edge` -> `compute_component`.
    let expanding_to = edge_ref.to;
    perform_entry_into_new_vertex(adapter, carrier, component, expanding_to, expanded)
}

pub(super) fn perform_entry_into_new_vertex<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    vertex: &IRVertex,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let mut stream = coerce_if_needed(adapter.as_ref(), carrier, vertex, stream);
    for filter_expr in &vertex.filters {
        stream = apply_local_field_filter(
            adapter.as_ref(),
            carrier,
            component,
            vertex.vid,
            filter_expr,
            stream,
        );
    }
    let vid = vertex.vid;
    stream = Box::pin(stream.map(move |result| {
        result.map(|mut context| {
            context.record_vertex(vid);
            context
        })
    }));
    stream
}

fn expand_non_recursive_edge<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    edge: &EdgeRef<'_>,
    is_optional: bool,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let (plain, upstream_error) = begin_stage(stream);

    // Re-activate the edge's source vertex before resolving neighbors. Without this, a second edge
    // expanded from an already-visited vertex would resolve neighbors of the *previous* edge's
    // destination instead (e.g. two `successor` edges off the same vertex). Mirrors the sync engine.
    let expanding_from_vid = edge.from.vid;
    let plain: ContextStream<'query, AdapterT::Vertex> =
        Box::pin(plain.map(move |context| context.activate_vertex(&expanding_from_vid)));

    let type_name = edge.from.type_name.clone();
    let edge_outcomes = carrier.resolve_edge_with(edge.from.vid, edge.to.vid, edge.eid, |info| {
        adapter.resolve_neighbors(plain, &type_name, edge.name, edge.parameters, info)
    });

    let staged = finish_stage(edge_outcomes, upstream_error);
    Box::pin(try_stream! {
        let mut staged = staged;
        while let Some(item) = staged.next().await {
            let (context, neighbors) = item?;
            let mut neighbors = neighbors;
            let mut has_neighbors = false;
            while let Some(neighbor) = neighbors.next().await {
                let vertex = neighbor?;
                has_neighbors = true;
                yield context.split_and_move_to_vertex(Some(vertex));
            }

            // If there's no current vertex, there couldn't possibly be neighbors.
            if context.active_vertex.is_none() {
                assert!(!has_neighbors);
            }

            // Emit a no-vertex context if we're inside a nonexistent `@optional` (so downstream
            // outputs become null), or if this optional edge itself had no neighbors.
            if context.active_vertex.is_none() || (!has_neighbors && is_optional) {
                yield context.split_and_move_to_vertex(None);
            }
        }
    })
}

/// Materialize a fold, returning `None` when it exceeds its maximum size.
async fn collect_fold_elements<'query, V, E>(
    mut stream: FallibleContextStream<'query, V, E>,
    max_fold_count_limit: &Option<usize>,
    min_fold_count_limit: &Option<usize>,
) -> Result<Option<Vec<DataContext<V>>>, E> {
    if let Some(max) = max_fold_count_limit {
        let mut elements = Vec::with_capacity((*max).min(16));
        let mut stopped_early = false;
        for _ in 0..*max {
            match stream.next().await {
                Some(item) => elements.push(item?),
                None => {
                    stopped_early = true;
                    break;
                }
            }
        }
        if !stopped_early && let Some(item) = stream.next().await {
            // Propagate an error even while discarding; otherwise the fold is over-size.
            item?;
            return Ok(None);
        }
        Ok(Some(elements))
    } else if let Some(min) = min_fold_count_limit {
        let mut elements = Vec::new();
        for _ in 0..*min {
            match stream.next().await {
                Some(item) => elements.push(item?),
                None => break,
            }
        }
        Ok(Some(elements))
    } else {
        let mut elements = Vec::new();
        while let Some(item) = stream.next().await {
            elements.push(item?);
        }
        Ok(Some(elements))
    }
}

/// Apply a post-fold filter to a fold-specific field such as the fold count.
#[allow(clippy::too_many_arguments)]
fn apply_fold_specific_filter<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    parent_component: &IRQueryComponent,
    current_vid: Vid,
    fold: &IRFold,
    filter: &Operation<FoldSpecificFieldKind, Argument>,
    query_arguments: &Arc<BTreeMap<Arc<str>, FieldValue>>,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let fold_eid = fold.eid;
    let kind = *filter.left();

    // Push the fold-specific field value (e.g. the count) onto each context's value stack.
    let with_value: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
        Box::pin(stream.map(move |result| {
            result.map(|mut context| {
                let value = match &kind {
                    FoldSpecificFieldKind::Count => {
                        match context.folded_contexts[&fold_eid].as_ref() {
                            Some(elements) => FieldValue::Uint64(elements.len() as u64),
                            None => unreachable!(
                                "post-fold filter reached a @fold inside a nonexistent @optional"
                            ),
                        }
                    }
                };
                context.values.push(value);
                context
            })
        }));

    // Tag-argument fold-count filters resolve the tag per context; delegate to the shared tag core.
    let filter_without_field = filter.map(|_| (), |r| r);
    if let Some(Argument::Tag(tag_ref)) = filter_without_field.right() {
        let tag_ref = tag_ref.clone();
        let op = ComparisonOp::from_binary_filter(&filter_without_field)
            .expect("tag fold filters are binary operations");
        return super::engine_filter::apply_tag_comparison(
            adapter,
            carrier,
            parent_component,
            current_vid,
            op,
            tag_ref,
            with_value,
        );
    }

    let predicate = if let Some(unary) = ValuePredicate::unary(&filter_without_field) {
        unary
    } else {
        match filter_without_field.right() {
            Some(Argument::Variable(var)) => {
                let right_value = query_arguments[var.variable_name.as_ref()].clone();
                ValuePredicate::static_argument(&filter_without_field, right_value)
            }
            Some(Argument::Tag(_)) => unreachable!("tag fold filters handled above"),
            None => unreachable!("non-unary fold filter with no argument"),
        }
    };

    filter_by_predicate(with_value, predicate)
}

/// Pop a filter value and retain matching contexts.
fn filter_by_predicate<'query, V, E>(
    stream: FallibleContextStream<'query, V, E>,
    predicate: ValuePredicate,
) -> FallibleContextStream<'query, V, E>
where
    V: Clone + Debug + 'query,
    E: 'query,
{
    Box::pin(stream.filter_map(move |result| {
        let outcome = match result {
            Ok(mut context) => {
                let left_value = context.values.pop().expect("no value present");
                (context.within_nonexistent_optional() || predicate.passes(&left_value))
                    .then_some(Ok(context))
            }
            Err(error) => Some(Err(error)),
        };
        std::future::ready(outcome)
    }))
}

/// Resolve outputs for one materialized fold.
async fn compute_fold_outputs<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    fold: &Arc<IRFold>,
    output_names: &[Arc<str>],
    expanding_from_vid: Vid,
    mut context: DataContext<AdapterT::Vertex>,
) -> Result<DataContext<AdapterT::Vertex>, AdapterT::Error> {
    let fold_eid = fold.eid;
    let fold_elements = &context.folded_contexts[&fold_eid];
    debug_assert_eq!(
        context.vertices[&expanding_from_vid].is_some(),
        fold_elements.is_some(),
        "mismatch on whether the fold below {expanding_from_vid:?} was inside an `@optional`",
    );

    // Fold-specific outputs (e.g. count). `null` (not empty) when inside a nonexistent `@optional`.
    for (output_name, fold_specific_field) in &fold.fold_specific_outputs {
        let value = fold_elements.as_ref().map(|elements| match fold_specific_field {
            FoldSpecificFieldKind::Count => {
                ValueOrVec::Value(FieldValue::Uint64(elements.len() as u64))
            }
        });
        context
            .folded_values
            .insert_or_error((fold_eid, output_name.clone()), value)
            .expect("this fold output was already computed");
    }

    let default_value = if fold_elements.is_some() { Some(ValueOrVec::Vec(vec![])) } else { None };
    let mut folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>> = output_names
        .iter()
        .map(|output| ((fold_eid, output.clone()), default_value.clone()))
        .collect();

    let fold_contains_elements = fold_elements.as_ref().map(|e| !e.is_empty()).unwrap_or(false);
    if !fold_contains_elements {
        // Ensure nested @fold outputs (recursively) get the default value too.
        let mut queue: Vec<_> = fold.component.folds.values().collect();
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
        let elements = fold_elements.as_ref().expect("fold did not contain elements").clone();
        let mut output_stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
            Box::pin(futures_util::stream::iter(
                elements.into_iter().map(Ok::<_, AdapterT::Error>),
            ));

        for output_name in output_names.iter() {
            let context_field = &fold.component.outputs[output_name.as_ref()];
            let vertex_id = context_field.vertex_id;

            let moved: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
                Box::pin(output_stream.map(move |result| {
                    result.map(|context| {
                        let new_vertex = context.vertices[&vertex_id].clone();
                        context.move_to_vertex(new_vertex)
                    })
                }));

            let (plain, upstream_error) = begin_stage(moved);
            let field_data = carrier.resolve_with(vertex_id, true, |info| {
                adapter.resolve_property(
                    plain,
                    &fold.component.vertices[&vertex_id].type_name,
                    &context_field.field_name,
                    info,
                )
            });

            let staged = finish_stage(field_data, upstream_error);
            output_stream = Box::pin(staged.map(|result| {
                result.map(|(mut context, value)| {
                    context.values.push(value);
                    context
                })
            }));
        }

        // Drain the resolved elements, appending values to the fold outputs in name order.
        while let Some(item) = output_stream.next().await {
            let mut folded_context = item?;
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

            // Values were pushed in increasing output-name order and popped from the back,
            // so iterate output names in reverse.
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
    }

    let prior_folded_values_count = context.folded_values.len();
    let new_folded_values_count = folded_values.len();
    context.folded_values.extend(folded_values);
    assert_eq!(
        context.folded_values.len(),
        prior_folded_values_count + new_folded_values_count,
        "fold output value maps had overlapping keys",
    );

    Ok(context)
}

#[allow(clippy::too_many_arguments)]
fn compute_fold<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    expanding_from: &IRVertex,
    parent_component: &IRQueryComponent,
    fold: Arc<IRFold>,
    mut stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    // === Imported tags needed inside the fold (eager construction). ===
    for imported_field in fold.imported_tags.iter() {
        match imported_field {
            FieldRef::ContextField(field) => {
                let vertex_id = field.vertex_id;
                let activated: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
                    Box::pin(stream.map(move |result| {
                        result.map(|context| context.activate_vertex(&vertex_id))
                    }));

                let type_name = parent_component.vertices[&field.vertex_id].type_name.clone();
                let (plain, upstream_error) = begin_stage(activated);
                let field_data = carrier.resolve_with(vertex_id, true, |info| {
                    adapter.resolve_property(plain, &type_name, &field.field_name, info)
                });

                let cloned_field = imported_field.clone();
                let staged = finish_stage(field_data, upstream_error);
                stream = Box::pin(staged.map(move |result| {
                    result.map(|(mut context, value)| {
                        let tag_value = if context.vertices[&vertex_id].is_some() {
                            TaggedValue::Some(value)
                        } else {
                            TaggedValue::NonexistentOptional
                        };
                        context.imported_tags.insert(cloned_field.clone(), tag_value);
                        context
                    })
                }));
            }
            FieldRef::FoldSpecificField(fold_specific_field) => {
                let cloned_field = imported_field.clone();
                let fold_eid = fold_specific_field.fold_eid;
                let kind = fold_specific_field.kind;
                stream = Box::pin(stream.map(move |result| {
                    result.map(|mut context| {
                        let tag_value = match &kind {
                            FoldSpecificFieldKind::Count => {
                                match context.folded_contexts[&fold_eid].as_ref() {
                                    None => TaggedValue::NonexistentOptional,
                                    Some(v) => {
                                        TaggedValue::Some(FieldValue::Uint64(v.len() as u64))
                                    }
                                }
                            }
                        };
                        context.imported_tags.insert(cloned_field.clone(), tag_value);
                        context
                    })
                }));
            }
        }
    }

    // === Resolve the fold edge to get the initial vertices inside the fold (eager). ===
    let expanding_from_vid = expanding_from.vid;
    let activated: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> = Box::pin(
        stream
            .map(move |result| result.map(|context| context.activate_vertex(&expanding_from_vid))),
    );
    let type_name = expanding_from.type_name.clone();
    let (plain, upstream_error) = begin_stage(activated);
    let edge_outcomes =
        carrier.resolve_edge_with(expanding_from_vid, fold.to_vid, fold.eid, |info| {
            adapter.resolve_neighbors(plain, &type_name, &fold.edge_name, &fold.parameters, info)
        });
    let edge_stream = finish_stage(edge_outcomes, upstream_error);

    // === Fold count limits (eager), mirroring the sync engine's optimization logic. ===
    let max_fold_size = get_max_fold_count_limit(carrier, fold.as_ref());
    let min_fold_size =
        if let Some(min_fold_size) = get_min_fold_count_limit(carrier, fold.as_ref()) {
            let no_outputs_in_fold = fold.component.outputs.is_empty();
            let has_output_on_fold_count =
                fold.fold_specific_outputs.values().any(|x| *x == FoldSpecificFieldKind::Count);
            let has_tag_on_fold_count = parent_component.vertices.values().any(|vertex| {
                vertex.filters.iter().any(|filter| {
                    let Some(Argument::Tag(FieldRef::FoldSpecificField(tagged_fold_count))) =
                        filter.right()
                    else {
                        return false;
                    };
                    tagged_fold_count.fold_root_vid == fold.to_vid
                        && tagged_fold_count.fold_eid == fold.eid
                        && tagged_fold_count.kind == FoldSpecificFieldKind::Count
                })
            });
            if no_outputs_in_fold && !has_output_on_fold_count && !has_tag_on_fold_count {
                Some(min_fold_size)
            } else {
                None
            }
        } else {
            None
        };

    let materialize_adapter = adapter.clone();
    let mut materialize_carrier = carrier.clone();
    let fold_component = fold.component.clone();
    let fold_eid = fold.eid;
    let moved_fold = fold.clone();

    // === Stage 1: materialize each fold and attach it to its parent context. ===
    let folded_stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> = Box::pin(
        try_stream! {
            let mut edge_stream = edge_stream;
            while let Some(item) = edge_stream.next().await {
                let (mut context, neighbors) = item?;
                let imported_tags = context.imported_tags.clone();

                let neighbor_contexts: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> = {
                    let imported_tags = imported_tags.clone();
                    Box::pin(neighbors.map(move |neighbor| {
                        neighbor.map(|vertex| {
                            let mut ctx = DataContext::new(Some(vertex));
                            ctx.imported_tags = imported_tags.clone();
                            ctx
                        })
                    }))
                };

                let computed = compute_component(
                    materialize_adapter.clone(),
                    &mut materialize_carrier,
                    &fold_component,
                    neighbor_contexts,
                );

                // A @fold inside a nonexistent @optional is `None`, not an empty `Some(vec)`.
                let fold_exists = context.vertices[&expanding_from_vid].is_some();
                let fold_elements = if fold_exists {
                    match collect_fold_elements(computed, &max_fold_size, &min_fold_size).await? {
                        Some(elements) => Some(elements),
                        // Over the max size — discard this context (mirrors the sync `?`).
                        None => continue,
                    }
                } else {
                    None
                };

                context.folded_contexts.insert_or_error(fold_eid, fold_elements).unwrap();

                for imported_tag in &moved_fold.imported_tags {
                    context.imported_tags.remove(imported_tag).unwrap();
                }

                yield context;
            }
        },
    );

    // === Stage 2: post-fold filters (e.g. on the fold count) (eager construction). ===
    let query_arguments = carrier.query.as_ref().expect("query was not returned").arguments.clone();
    let mut post_filtered = folded_stream;
    for post_fold_filter in fold.post_filters.iter() {
        post_filtered = apply_fold_specific_filter(
            adapter.as_ref(),
            carrier,
            parent_component,
            expanding_from_vid,
            fold.as_ref(),
            post_fold_filter,
            &query_arguments,
            post_filtered,
        );
    }

    // === Stage 3: compute the fold's outputs for each surviving context. ===
    let mut output_names: Vec<Arc<str>> = fold.component.outputs.keys().cloned().collect();
    output_names.sort_unstable(); // deterministic resolve_property() ordering
    let output_names = Arc::new(output_names);

    let output_adapter = adapter.clone();
    let mut output_carrier = carrier.clone();
    let output_fold = fold.clone();

    Box::pin(try_stream! {
        let mut post_filtered = post_filtered;
        while let Some(item) = post_filtered.next().await {
            let context = item?;
            let output_context = compute_fold_outputs(
                &output_adapter,
                &mut output_carrier,
                &output_fold,
                &output_names,
                expanding_from_vid,
                context,
            )
            .await?;
            yield output_context;
        }
    })
}

#[allow(clippy::type_complexity)]
fn construct_outputs<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> Pin<Box<dyn Stream<Item = Result<BTreeMap<Arc<str>, FieldValue>, AdapterT::Error>> + 'query>> {
    let query = carrier.query.take().expect("query was not returned");

    let root_component = query.indexed_query.ir_query.root_component.clone();
    let mut output_names: Vec<Arc<str>> = root_component.outputs.keys().cloned().collect();
    output_names.sort_unstable(); // deterministic resolve_property() ordering

    carrier.query = Some(query);
    let mut output_stream = stream;

    for output_name in output_names.iter() {
        let context_field = &root_component.outputs[output_name];
        let vertex_id = context_field.vertex_id;

        let moved_stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
            Box::pin(output_stream.map(move |result| {
                result.map(|context| {
                    let new_vertex = context.vertices[&vertex_id].clone();
                    context.move_to_vertex(new_vertex)
                })
            }));

        let (plain, upstream_error) = begin_stage(moved_stream);

        let field_data = carrier.resolve_with(vertex_id, true, |info| {
            adapter.resolve_property(
                plain,
                &root_component.vertices[&vertex_id].type_name,
                &context_field.field_name,
                info,
            )
        });

        let staged = finish_stage(field_data, upstream_error);
        output_stream = Box::pin(staged.map(|result| {
            result.map(|(mut context, value)| {
                context.values.push(value);
                context
            })
        }));
    }

    let query = carrier.query.as_ref().expect("query was not returned");
    let expected_output_names: BTreeSet<Arc<str>> =
        query.indexed_query.outputs.keys().cloned().collect();

    let output_names = Arc::new(output_names);
    Box::pin(output_stream.map(move |result| {
        result.map(|mut context| {
            assert!(
                context.values.len() == output_names.len(),
                "expected {output_names:?} but got {:?}",
                context.values
            );

            let mut output: BTreeMap<Arc<str>, FieldValue> =
                output_names.iter().cloned().zip(context.values.drain(..)).collect();

            for ((_, output_name), output_value) in context.folded_values {
                let existing = output.insert(output_name, output_value.into());
                assert!(existing.is_none());
            }

            debug_assert_eq!(
                expected_output_names,
                output.keys().cloned().collect::<BTreeSet<_>>()
            );

            output
        })
    }))
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;

    use futures_util::StreamExt;

    use super::{begin_stage, finish_stage};
    use crate::interpreter::{ContextOutcomeStream, DataContext};

    type Upstream =
        Pin<Box<dyn futures_core::Stream<Item = Result<DataContext<i32>, u8>> + 'static>>;

    fn ctx(v: i32) -> DataContext<i32> {
        DataContext::new(Some(v))
    }

    fn upstream(items: Vec<Result<DataContext<i32>, u8>>) -> Upstream {
        Box::pin(futures_util::stream::iter(items))
    }

    type Staged =
        Pin<Box<dyn futures_core::Stream<Item = Result<(DataContext<i32>, &'static str), u8>>>>;

    /// Compose a stage exactly like the engine does: the adapter's resolver consumes the
    /// plain contexts and produces one fallible outcome per context it saw.
    fn staged(
        input: Upstream,
        mut resolver: impl FnMut(DataContext<i32>) -> Result<&'static str, u8> + 'static,
    ) -> Staged {
        let (plain, continuation) = begin_stage(input);
        let outcomes: ContextOutcomeStream<'static, i32, Result<&'static str, u8>> =
            Box::pin(plain.map(move |ctx| (ctx.clone(), resolver(ctx))));
        finish_stage(outcomes, continuation)
    }

    /// The adapter's error takes precedence over a later upstream error, and rows before
    /// the error still flow: fail-fast means "first error wins, then the stream ends".
    #[test]
    fn adapter_error_precedes_later_upstream_error() {
        let stream = staged(upstream(vec![Ok(ctx(1)), Ok(ctx(2)), Err(9), Ok(ctx(3))]), |ctx| {
            match ctx.active_vertex {
                Some(2) => Err(7),
                Some(_) => Ok("a"),
                None => Ok("n"),
            }
        });
        let items: Vec<_> = futures_executor::block_on(stream.collect());
        assert_eq!(items, vec![Ok((ctx(1), "a")), Err(7)]);
    }

    /// An upstream error surfaces exactly once, after all successfully-resolved outcomes.
    #[test]
    fn upstream_error_surfaces_after_outcomes() {
        let stream = staged(upstream(vec![Ok(ctx(1)), Ok(ctx(2)), Err(9), Ok(ctx(3))]), |ctx| {
            match ctx.active_vertex {
                Some(2) => Ok("b"),
                Some(_) => Ok("a"),
                None => Ok("n"),
            }
        });
        let items: Vec<_> = futures_executor::block_on(stream.collect());
        assert_eq!(items, vec![Ok((ctx(1), "a")), Ok((ctx(2), "b")), Err(9)]);
    }

    /// Terminal-on-error contract (as in DataFusion's stream docs): after yielding an
    /// error the stream returns `None` on every subsequent poll.
    #[test]
    fn stream_is_terminal_after_error() {
        let mut stream = staged(upstream(vec![Err(1)]), |_| unreachable!("no contexts to resolve"));
        assert_eq!(futures_executor::block_on(stream.next()), Some(Err(1)));
        assert_eq!(futures_executor::block_on(stream.next()), None);
        assert_eq!(futures_executor::block_on(stream.next()), None);
    }

    /// The stream is fused after successful completion as well.
    #[test]
    fn stream_is_terminal_after_completion() {
        let mut stream = staged(upstream(vec![Ok(ctx(1))]), |_| Ok("done"));
        assert_eq!(futures_executor::block_on(stream.next()), Some(Ok((ctx(1), "done"))));
        assert_eq!(futures_executor::block_on(stream.next()), None);
        assert_eq!(futures_executor::block_on(stream.next()), None);
    }
}
