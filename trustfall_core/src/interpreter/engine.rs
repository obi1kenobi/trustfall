//! Trustfall's runtime-agnostic, `Stream`-native execution kernel.
//!
//! Both public execution routes enter here. Native async adapters may suspend while resolving
//! data; the synchronous frontend supplies streams that are always ready and projects the output
//! back to an iterator. Query semantics therefore have exactly one implementation.
//!
//! # Strongly-typed, native error threading
//!
//! The engine's internal streams carry `Result<DataContext<V>, E>` ([`FallibleContextStream`]) and
//! fail fast on the first `Err` using `?` inside [`async_stream::try_stream!`]. Adapter resolvers,
//! however, take *plain* context streams and return `Result<(context, outcome), Error>` items.
//! [`begin_stage`]
//! temporarily projects successful contexts into that public resolver protocol and returns a
//! linear [`StageContinuation`] token. Consuming that token in [`finish_stage`] reconnects the
//! upstream error path. This preserves both the adapter's one-outcome-per-context contract and
//! fail-fast semantics without exposing prior-stage errors to adapter implementations.
//!
//! # Construction is synchronous
//!
//! Just like the sync engine, building the pipeline (the `carrier`/`ResolveInfo` dance and the
//! adapter resolver *calls*) happens eagerly and synchronously; only *consuming* the composed
//! stream is async. Recursion-during-iteration (`@fold`/`@recurse`) uses cloned carriers, exactly
//! as the sync engine does.

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    pin::Pin,
    rc::Rc,
    sync::Arc,
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
    DataContext, InterpretedQuery, ResolveEdgeInfo, ResolveInfo, TaggedValue, ValueOrVec,
    async_adapter::{AsyncAdapter, ContextOutcomeStream, ContextStream},
    error::{ExecutionError, QueryArgumentsError},
    execution::{QueryCarrier, get_max_fold_count_limit, get_min_fold_count_limit},
    filtering::{static_argument_filter_predicate, unary_filter_predicate},
};

/// A stream of contexts that may fail: the engine's internal, strongly-typed representation.
/// Fail-fast is expressed as an `Err` item, after which the stream ends.
pub type FallibleContextStream<'vertex, VertexT, E> =
    Pin<Box<dyn Stream<Item = Result<DataContext<VertexT>, E>> + 'vertex>>;

/// Proof that a resolver stage's upstream error path still needs to be reconnected.
///
/// The shared cell is an implementation detail of projecting a fallible stream into the
/// adapter-facing plain-context protocol. The token itself is linear: finishing a stage consumes
/// it, and no shared error state escapes the execution kernel.
#[must_use = "a resolver stage continuation must be passed to finish_stage"]
pub(super) struct StageContinuation<E> {
    pending_error: Rc<RefCell<Option<E>>>,
}

impl<E> StageContinuation<E> {
    fn take_error(self) -> Option<E> {
        self.pending_error.borrow_mut().take()
    }
}

/// Execute an indexed query as a stream.
///
/// Returns rows (or the first execution error, fail-fast). Query parsing and
/// argument validation still fail eagerly via the outer `Result<_, QueryArgumentsError>`.
///
/// The returned stream is lazy: adapter code runs only as it is polled.
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

/// Peel the successful contexts off a fallible stream for feeding an adapter, capturing the first
/// upstream error so a later stage can re-surface it. See the module docs.
pub(super) fn begin_stage<'vertex, V, E>(
    input: FallibleContextStream<'vertex, V, E>,
) -> (ContextStream<'vertex, V>, StageContinuation<E>)
where
    V: Clone + Debug + 'vertex,
    E: 'vertex,
{
    let pending_error = Rc::new(RefCell::new(None));
    let plain: ContextStream<'vertex, V> = {
        let pending_error = pending_error.clone();
        Box::pin(async_stream::stream! {
            let mut input = input;
            while let Some(item) = input.next().await {
                match item {
                    Ok(ctx) => yield ctx,
                    Err(error) => {
                        *pending_error.borrow_mut() = Some(error);
                        break;
                    }
                }
            }
        })
    };
    (plain, StageContinuation { pending_error })
}

/// Thread an adapter resolver's fallible outcome stream into the engine, failing fast on the
/// adapter's own errors and then re-surfacing the upstream error captured by [`begin_stage`].
#[allow(clippy::type_complexity)]
pub(super) fn finish_stage<'vertex, V, O, E>(
    outcomes: ContextOutcomeStream<'vertex, V, O, E>,
    continuation: StageContinuation<E>,
) -> Pin<Box<dyn Stream<Item = Result<(DataContext<V>, O), E>> + 'vertex>>
where
    V: 'vertex,
    O: 'vertex,
    E: 'vertex,
{
    Box::pin(try_stream! {
        let mut outcomes = outcomes;
        while let Some(outcome) = outcomes.next().await {
            yield outcome?;
        }
        // Bind before the branch so the `RefMut` is dropped and never held across a yield/await.
        let pending = continuation.take_error();
        if let Some(error) = pending {
            Err(error)?;
        }
    })
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

        assert!(process_next_fold.is_some() ^ process_next_edge.is_some());

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

/// Resolve a `@filter`'s local field value and drop contexts that fail the filter.
///
/// Reuses the sync engine's filter predicates (see [`super::filtering`]) so the two engines apply
/// identical semantics. Unary, static-argument, and `@tag`-argument filters are all supported
/// (tag filters via [`super::engine_filter`]).
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
    let (plain, upstream_error) = begin_stage(stream);
    let type_name = component.vertices[&current_vid].type_name.clone();
    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveInfo::new(query, current_vid, true);
    let field_data =
        adapter.resolve_property(plain, &type_name, &local_field.field_name, &resolve_info);
    carrier.query = Some(resolve_info.into_inner());

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
    let predicate: Box<dyn Fn(&FieldValue) -> bool> = if let Some(unary) =
        unary_filter_predicate(&filter_without_field)
    {
        Box::new(unary)
    } else {
        match filter_without_field.right() {
            Some(Argument::Variable(var)) => {
                let right_value = carrier.query.as_ref().expect("query was not returned").arguments
                    [var.variable_name.as_ref()]
                .clone();
                static_argument_filter_predicate(&filter_without_field, right_value)
            }
            Some(Argument::Tag(_)) => unreachable!("tag filters handled above"),
            None => unreachable!("non-unary filter with no argument: {filter_without_field:?}"),
        }
    };

    Box::pin(with_value.filter_map(move |result| {
        let outcome = match result {
            Ok(mut context) => {
                let left_value = context.values.pop().expect("no value present");
                // Keep the context if it's inside a nonexistent `@optional` (filter is vacuous
                // there) or if the predicate passes.
                (context.within_nonexistent_optional() || predicate(&left_value))
                    .then_some(Ok(context))
            }
            Err(error) => Some(Err(error)),
        };
        std::future::ready(outcome)
    }))
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

    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveInfo::new(query, vertex.vid, false);
    let coercion_outcomes = adapter.resolve_coercion(plain, coerced_from, coerce_to, &resolve_info);
    carrier.query = Some(resolve_info.into_inner());

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
    let expanded = if let Some(recursive) = &edge.recursive {
        super::engine_recurse::expand_recursive_edge(
            adapter.clone(),
            carrier,
            component,
            &component.vertices[&expanding_from_vid],
            &component.vertices[&expanding_to_vid],
            edge.eid,
            &edge.edge_name,
            &edge.parameters,
            recursive,
            stream,
        )
    } else {
        expand_non_recursive_edge(
            adapter.as_ref(),
            carrier,
            &component.vertices[&expanding_from_vid],
            &component.vertices[&expanding_to_vid],
            edge.eid,
            &edge.edge_name,
            &edge.parameters,
            edge.optional,
            stream,
        )
    };

    // Recurse into the neighboring vertex's own component processing (coercions, filters,
    // sub-edges), exactly as the sync engine does via `expand_edge` -> `compute_component`.
    let expanding_to = &component.vertices[&expanding_to_vid];
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

#[allow(clippy::too_many_arguments)]
pub(super) fn expand_non_recursive_edge<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &EdgeParameters,
    is_optional: bool,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let (plain, upstream_error) = begin_stage(stream);

    // Re-activate the edge's source vertex before resolving neighbors. Without this, a second edge
    // expanded from an already-visited vertex would resolve neighbors of the *previous* edge's
    // destination instead (e.g. two `successor` edges off the same vertex). Mirrors the sync engine.
    let expanding_from_vid = expanding_from.vid;
    let plain: ContextStream<'query, AdapterT::Vertex> =
        Box::pin(plain.map(move |context| context.activate_vertex(&expanding_from_vid)));

    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveEdgeInfo::new(query, expanding_from.vid, expanding_to.vid, edge_id);
    let edge_outcomes = adapter.resolve_neighbors(
        plain,
        &expanding_from.type_name,
        edge_name,
        edge_parameters,
        &resolve_info,
    );
    carrier.query = Some(resolve_info.into_inner());

    let staged = finish_stage(edge_outcomes, upstream_error);
    Box::pin(try_stream! {
        let mut staged = staged;
        while let Some(item) = staged.next().await {
            let (context, neighbors) = item?;
            let mut has_neighbors = false;
            let mut neighbors = neighbors;
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

/// Drain a fold sub-pipeline into its materialized elements, honoring the same early-termination
/// limits as the sync [`collect_fold_elements`](super::execution). Returns `Ok(None)` when the fold
/// exceeds its max size (the caller discards that context), or the first `Err` (fail-fast).
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

/// Apply a post-fold `@filter` on a fold-specific field (e.g. the fold count). Mirrors the sync
/// [`apply_fold_specific_filter`](super::execution): the fold-specific value is computed from the
/// already-materialized `folded_contexts` (no adapter call), pushed as the left value, then the
/// filter predicate is applied. `@tag`-argument fold-count filters are not yet supported.
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
        let filter_owned = filter.map(|_| (), |r| r.clone());
        return super::engine_filter::apply_tag_comparison(
            adapter,
            carrier,
            parent_component,
            current_vid,
            filter_owned,
            tag_ref,
            with_value,
        );
    }

    let predicate: Box<dyn Fn(&FieldValue) -> bool> =
        if let Some(unary) = unary_filter_predicate(&filter_without_field) {
            Box::new(unary)
        } else {
            match filter_without_field.right() {
                Some(Argument::Variable(var)) => {
                    let right_value = query_arguments[var.variable_name.as_ref()].clone();
                    static_argument_filter_predicate(&filter_without_field, right_value)
                }
                Some(Argument::Tag(_)) => unreachable!("tag fold filters handled above"),
                None => unreachable!("non-unary fold filter with no argument"),
            }
        };

    filter_by_predicate(with_value, predicate)
}

/// Shared final step of a value-based `@filter`: pop the pushed left value and keep the context if
/// it's inside a nonexistent `@optional` or the predicate passes. Errors pass through (fail-fast).
fn filter_by_predicate<'query, V, E>(
    stream: FallibleContextStream<'query, V, E>,
    predicate: Box<dyn Fn(&FieldValue) -> bool>,
) -> FallibleContextStream<'query, V, E>
where
    V: Clone + Debug + 'query,
    E: 'query,
{
    Box::pin(stream.filter_map(move |result| {
        let outcome = match result {
            Ok(mut context) => {
                let left_value = context.values.pop().expect("no value present");
                (context.within_nonexistent_optional() || predicate(&left_value))
                    .then_some(Ok(context))
            }
            Err(error) => Some(Err(error)),
        };
        std::future::ready(outcome)
    }))
}

/// Resolve a fold's output properties for a single context from its materialized elements, building
/// the `folded_values` map. Mirrors the output-computation tail of the sync `compute_fold`.
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
            let query = carrier.query.take().expect("query was not returned");
            let resolve_info = ResolveInfo::new(query, vertex_id, true);
            let field_data = adapter.resolve_property(
                plain,
                &fold.component.vertices[&vertex_id].type_name,
                &context_field.field_name,
                &resolve_info,
            );
            carrier.query = Some(resolve_info.into_inner());

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
                let query = carrier.query.take().expect("query was not returned");
                let resolve_info = ResolveInfo::new(query, vertex_id, true);
                let field_data =
                    adapter.resolve_property(plain, &type_name, &field.field_name, &resolve_info);
                carrier.query = Some(resolve_info.into_inner());

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
    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveEdgeInfo::new(query, expanding_from_vid, fold.to_vid, fold.eid);
    let edge_outcomes = adapter.resolve_neighbors(
        plain,
        &type_name,
        &fold.edge_name,
        &fold.parameters,
        &resolve_info,
    );
    carrier.query = Some(resolve_info.into_inner());
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
    let mut query = carrier.query.take().expect("query was not returned");

    let root_component = query.indexed_query.ir_query.root_component.clone();
    let mut output_names: Vec<Arc<str>> = root_component.outputs.keys().cloned().collect();
    output_names.sort_unstable(); // deterministic resolve_property() ordering

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

        let resolve_info = ResolveInfo::new(query, vertex_id, true);
        let type_name = &root_component.vertices[&vertex_id].type_name;
        let field_data =
            adapter.resolve_property(plain, type_name, &context_field.field_name, &resolve_info);
        query = resolve_info.into_inner();

        let staged = finish_stage(field_data, upstream_error);
        output_stream = Box::pin(staged.map(|result| {
            result.map(|(mut context, value)| {
                context.values.push(value);
                context
            })
        }));
    }

    let expected_output_names: BTreeSet<Arc<str>> =
        query.indexed_query.outputs.keys().cloned().collect();
    carrier.query = Some(query);

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
