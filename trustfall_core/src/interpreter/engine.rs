//! The shared stream execution kernel.
//!
//! Async adapters suspend while resolving data. The synchronous frontend supplies ready streams
//! and projects results back to an iterator. Both use the same execution stages.
//!
//! Internal streams carry adapter errors, but adapter resolvers receive only plain contexts.
//! [`begin_stage`] stops before an upstream error and saves it. [`finish_stage`] yields resolver
//! outcomes first, then yields that saved error. This keeps the execution stream fail-fast without
//! making every adapter implementation handle an error channel it did not create.

use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
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

/// An internal context stream whose error is terminal.
///
/// A stage either yields successful contexts until it reaches an adapter error, or it ends.
/// Downstream stages rely on this to avoid invoking an adapter after a failed earlier stage.
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

/// The saved upstream error path for one resolver stage.
///
/// [`begin_stage`] produces this together with a plain context stream. It must be passed to
/// [`finish_stage`] after the adapter's resolver has been called, otherwise an upstream error
/// would be silently lost.
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
///
/// An adapter error wins immediately. Otherwise, after the resolver has drained every context it
/// received, the saved error from the preceding stage becomes the final item in the stream.
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
    V: 'vertex,
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
///
/// Query argument validation and the starting-vertex resolver call happen while building the
/// stream. Every later resolver call remains lazy: it runs only when the caller polls for more
/// results. This is the same boundary exposed by the synchronous interpreter.
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

    // `ResolveInfo` owns the query while the adapter is called. Put the query back before building
    // downstream stages: each resolver stage takes it out again for its own `ResolveInfo`.
    let starting =
        adapter.resolve_starting_vertices(&root_edge, &root_edge_parameters, &resolve_info);
    carrier.query = Some(resolve_info.into_inner());

    let mut stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
        Box::pin(starting.map(|result| result.map(|vertex| DataContext::new(Some(vertex)))));

    let component = ir_query.root_component.clone();
    stream = compute_component(adapter.clone(), &mut carrier, &component, stream);

    let outputs = construct_outputs(adapter.as_ref(), &mut carrier, stream);

    Ok(Box::pin(outputs.map(|result| result.map_err(ExecutionError::Adapter))))
}

pub(super) fn compute_component<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    mut stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let component_root_vid = component.root;

    // A component root and an edge destination enter the component the same way. Keeping that
    // setup in `prepare_vertex` makes coercions, filters, and vertex bookkeeping identical.
    stream = prepare_vertex(
        adapter.clone(),
        carrier,
        component,
        &component.vertices[&component_root_vid],
        stream,
    );

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
///
/// The resolved left value is pushed onto `DataContext::values` until the filter consumes it.
/// Static arguments can then use a pure predicate. Tag arguments need a second, per-context
/// lookup, so the tag-filter module takes over after the left value has been recorded.
fn apply_local_field_filter<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    filter: &Operation<LocalField, Argument>,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let local_field = filter.left();

    // Resolve the filter's left side first. This stack entry survives a tag lookup, where the
    // active vertex may temporarily move to the tagged field's vertex.
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

    let predicate = build_value_predicate(carrier, &filter_without_field);

    filter_by_predicate(with_value, predicate)
}

/// Construct a predicate for a unary or runtime-argument filter.
///
/// Tags are deliberately excluded. Unlike variables, a tag can name a different vertex and must
/// therefore be resolved against each individual context.
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
            // A nonexistent `@optional` has no vertex to test. Preserve it so later output
            // resolution produces `null` instead of removing the outer context.
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

    // Neighbor contexts have arrived at the destination. Process that vertex before expanding
    // any edge below it, just as `compute_component` processes a component root.
    prepare_vertex(adapter, carrier, component, edge_ref.to, expanded)
}

/// Apply a vertex's entry conditions and retain it in the context.
///
/// Every context that reaches a vertex is coerced, filtered, then recorded under its vertex ID.
/// Recording last matters: filters and coercions operate on the active vertex, while later edges
/// reactivate this stored value when they return to the vertex.
fn prepare_vertex<'query, AdapterT: AsyncAdapter<'query> + 'query>(
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

    // Component execution leaves each context at the most recently visited vertex. An edge must
    // start at its declared source, especially when several sibling edges share that source.
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

            if context.active_vertex.is_none() {
                assert!(!has_neighbors);
            }

            // There are no neighbors beneath a nonexistent optional vertex. Preserve the absent
            // vertex, and do the same for an optional edge with no neighbors, so descendants and
            // outputs observe the optional scope as `null` rather than losing the parent row.
            if context.active_vertex.is_none() || (!has_neighbors && is_optional) {
                yield context.split_and_move_to_vertex(None);
            }
        }
    })
}

/// Materialize a fold, returning `None` when it exceeds its maximum size.
///
/// `Some(vec![])` is an existing empty fold. `None` is reserved for an over-limit fold, which
/// causes its parent context to be discarded before any post-fold filters run.
async fn collect_fold_elements<'query, V, E>(
    mut stream: FallibleContextStream<'query, V, E>,
    max_fold_count_limit: &Option<usize>,
    min_fold_count_limit: &Option<usize>,
) -> Result<Option<Vec<DataContext<V>>>, E> {
    if let Some(max) = max_fold_count_limit {
        let mut elements = Vec::with_capacity((*max).min(16));
        for _ in 0..*max {
            let Some(item) = stream.next().await else {
                return Ok(Some(elements));
            };
            elements.push(item?);
        }
        if let Some(item) = stream.next().await {
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
///
/// Like a local filter, this pushes the left value onto the context stack and delegates tag
/// arguments to the tag-filter stage. The field is available only after the fold has been
/// materialized, which is why these filters run after edge resolution and component execution.
#[allow(clippy::too_many_arguments)]
fn apply_fold_specific_filter<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    parent_component: &IRQueryComponent,
    current_vid: Vid,
    fold: &IRFold,
    filter: &Operation<FoldSpecificFieldKind, Argument>,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let fold_eid = fold.eid;
    let kind = *filter.left();

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

    let predicate = build_value_predicate(carrier, &filter_without_field);

    filter_by_predicate(with_value, predicate)
}

/// Pop a filter value and retain matching contexts.
///
/// Each caller has pushed exactly one left value. A nonexistent `@optional` passes filters
/// vacuously, because there is no inner value that should decide the fate of the outer context.
fn filter_by_predicate<'query, V, E>(
    stream: FallibleContextStream<'query, V, E>,
    predicate: ValuePredicate,
) -> FallibleContextStream<'query, V, E>
where
    V: 'query,
    E: 'query,
{
    Box::pin(stream.filter_map(move |result| {
        std::future::ready(match result {
            Ok(mut context) => {
                let left_value = context.values.pop().expect("no value present");
                (context.within_nonexistent_optional() || predicate.passes(&left_value))
                    .then_some(Ok(context))
            }
            Err(error) => Some(Err(error)),
        })
    }))
}

/// Resolve outputs for one materialized fold.
///
/// Fold outputs are accumulated as vectors in the order their element contexts were produced.
/// An absent fold keeps those outputs `null`; an existing empty fold produces empty vectors.
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

    let fold_contains_elements =
        fold_elements.as_ref().is_some_and(|elements| !elements.is_empty());
    if !fold_contains_elements {
        // Nested folds cannot run without an outer element. Seed their declared outputs now so
        // their shape still agrees with the query, even though no child context is evaluated.
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

        while let Some(item) = output_stream.next().await {
            let mut folded_context = item?;

            // Nested fold values were computed while resolving this element. Append them before
            // consuming the ordinary output stack so both kinds retain element order.
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

            // Properties were resolved in name order and pushed in that order. Pop in reverse to
            // attach every value to the output name that requested it.
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
    // A nested component starts with fresh contexts, so values tagged outside the fold must be
    // copied in before the fold edge is resolved. They are removed again once the fold is done.
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

    // A fold, like an ordinary edge, starts from its declared source rather than whichever vertex
    // the component happened to visit most recently.
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

                // A fold under a nonexistent optional is absent, not an empty collection. That
                // distinction is observable in both fold-specific outputs and tagged filters.
                let fold_exists = context.vertices[&expanding_from_vid].is_some();
                let fold_elements = if fold_exists {
                    match collect_fold_elements(computed, &max_fold_size, &min_fold_size).await? {
                        Some(elements) => Some(elements),
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

    // Post-fold filters observe materialized fields such as `@fold` count.
    let mut post_filtered = folded_stream;
    for post_fold_filter in fold.post_filters.iter() {
        post_filtered = apply_fold_specific_filter(
            adapter.as_ref(),
            carrier,
            parent_component,
            expanding_from_vid,
            fold.as_ref(),
            post_fold_filter,
            post_filtered,
        );
    }

    let mut output_names: Vec<Arc<str>> = fold.component.outputs.keys().cloned().collect();
    // Resolver calls are ordered for deterministic adapter behavior and stack consumption.
    output_names.sort_unstable();
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
    // Resolve each property in a stable order; the values are later drained in the same order.
    output_names.sort_unstable();

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
