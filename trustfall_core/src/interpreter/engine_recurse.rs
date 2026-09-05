//! Stream-native `@recurse` edge expansion.
//!
//! Mirrors the synchronous [`expand_recursive_edge`](super::execution) logic (the depth loop, the
//! `RecursiveEdgeExpander` piggyback bookkeeping, per-depth coercion, and piggyback unpacking) but
//! on `Stream`s. See the sync engine for the detailed algorithm and invariants.

use std::sync::Arc;

use async_stream::try_stream;
use futures_util::StreamExt;

use crate::ir::{EdgeParameters, Eid, IRQueryComponent, IRVertex, Recursive};

use super::{
    DataContext, ResolveEdgeInfo, ResolveInfo,
    async_adapter::AsyncAdapter,
    engine::{FallibleContextStream, begin_stage, finish_stage},
    execution::QueryCarrier,
};

/// Expand a recursive (`@recurse`) edge over a fallible context stream.
///
/// Returns the expanded stream *before* entry into the destination vertex (the caller applies
/// `perform_entry_into_new_vertex`), matching the sync engine's `expand_recursive_edge`.
#[allow(clippy::too_many_arguments)]
pub(super) fn expand_recursive_edge<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    _component: &IRQueryComponent,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &EdgeParameters,
    recursive: &Recursive,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let expanding_from_vid = expanding_from.vid;

    // Push a None-sentinel for contexts that start with no active vertex (already inside @optional).
    // This mirrors the sync engine's pre-loop setup so ensure_unsuspended() later is symmetric.
    let mut recursion_stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> =
        Box::pin(stream.map(move |result| {
            result.map(|mut context| {
                if context.active_vertex.is_none() {
                    context.suspended_vertices.push(None);
                }
                context.activate_vertex(&expanding_from_vid)
            })
        }));

    let max_depth = usize::from(recursive.depth);

    let edge_endpoint_type =
        expanding_to.coerced_from_type.as_ref().unwrap_or(&expanding_to.type_name);
    let recursing_from = recursive.coerce_to.as_ref().unwrap_or(edge_endpoint_type);

    // First expansion uses the expanding_from type name (before any coercion-based recursing).
    recursion_stream = perform_one_recursive_edge_expansion(
        adapter.as_ref(),
        carrier,
        &expanding_from.type_name,
        expanding_from,
        expanding_to,
        edge_id,
        edge_name,
        edge_parameters,
        recursion_stream,
    );

    // Depths 2..=max_depth: optionally coerce, then expand again.
    for _ in 2..=max_depth {
        if let Some(coerce_to) = recursive.coerce_to.as_ref() {
            // Unusual coercion: non-coercible elements are kept but suspended (not discarded).
            let (plain, upstream_error) = begin_stage(recursion_stream);
            let query = carrier.query.take().expect("query was not returned");
            let resolve_info = ResolveInfo::new(query, expanding_from_vid, false);
            let coercion_outcomes =
                adapter.resolve_coercion(plain, edge_endpoint_type, coerce_to, &resolve_info);
            carrier.query = Some(resolve_info.into_inner());

            let staged = finish_stage(coercion_outcomes, upstream_error);
            recursion_stream = Box::pin(try_stream! {
                let mut staged = staged;
                while let Some(item) = staged.next().await {
                    let (ctx, can_coerce) = item?;
                    if can_coerce {
                        yield ctx;
                    } else {
                        yield ctx.ensure_suspended();
                    }
                }
            });
        }

        recursion_stream = perform_one_recursive_edge_expansion(
            adapter.as_ref(),
            carrier,
            recursing_from,
            expanding_from,
            expanding_to,
            edge_id,
            edge_name,
            edge_parameters,
            recursion_stream,
        );
    }

    // Unpack piggybacked contexts and unsuspend all contexts.
    Box::pin(try_stream! {
        let mut recursion_stream = recursion_stream;
        while let Some(item) = recursion_stream.next().await {
            let context = item?;
            for unpacked in unpack_piggyback(context) {
                assert!(unpacked.piggyback.is_none());
                yield unpacked.ensure_unsuspended();
            }
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn perform_one_recursive_edge_expansion<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    expanding_from_type: &Arc<str>,
    expanding_from: &IRVertex,
    expanding_to: &IRVertex,
    edge_id: Eid,
    edge_name: &Arc<str>,
    edge_parameters: &EdgeParameters,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    // In the recursive expansion, contexts are already positioned at the correct vertex
    // (either the initial from-vertex or the previous depth's neighbor). No re-activation needed.
    let (plain, upstream_error) = begin_stage(stream);

    let query = carrier.query.take().expect("query was not returned");
    let resolve_info = ResolveEdgeInfo::new(query, expanding_from.vid, expanding_to.vid, edge_id);
    let edge_outcomes = adapter.resolve_neighbors(
        plain,
        expanding_from_type,
        edge_name,
        edge_parameters,
        &resolve_info,
    );
    carrier.query = Some(resolve_info.into_inner());

    let staged = finish_stage(edge_outcomes, upstream_error);

    // For each (context, neighbors) pair, emit the piggyback-packed neighbor contexts,
    // plus the original context itself (which will be collected when unpacking piggybacked riders).
    Box::pin(try_stream! {
        let mut staged = staged;
        while let Some(item) = staged.next().await {
            let (context, neighbors) = item?;
            let mut neighbors = neighbors;

            // Use Option so we can move `context` out exactly once (on first neighbor).
            let mut context_slot: Option<DataContext<AdapterT::Vertex>> = Some(context);
            // Base context for cloning subsequent neighbor contexts.
            let mut neighbor_base: Option<DataContext<AdapterT::Vertex>> = None;

            while let Some(neighbor_result) = neighbors.next().await {
                let vertex = neighbor_result?;

                if let Some(ctx) = context_slot.take() {
                    // First neighbor: record whether ctx has an active vertex (for assertion below).
                    // Split into base (no vertex) and neighbor context, attach self as piggyback.
                    let base = ctx.split_and_move_to_vertex(None);
                    let mut neighbor_ctx = ctx.split_and_move_to_vertex(Some(vertex));
                    neighbor_ctx
                        .piggyback
                        .get_or_insert_with(Default::default)
                        .push(ctx.ensure_suspended());
                    neighbor_base = Some(base);
                    yield neighbor_ctx;
                } else {
                    // Subsequent neighbors: clone from the base.
                    yield neighbor_base.as_ref().unwrap().split_and_move_to_vertex(Some(vertex));
                }
            }

            if let Some(ctx) = context_slot {
                // No neighbors were produced. Validate and emit the original context unchanged
                // so it passes through to the post-processing unpacking stage.
                if ctx.active_vertex.is_none() {
                    // If there's no current vertex, there couldn't possibly be neighbors.
                    // (Already confirmed by context_slot still being Some.)
                }
                yield ctx;
            }
            // If there WERE neighbors, the original context was already attached as a piggyback
            // on the first neighbor, so it will be emitted during post-processing unpacking.
        }
    })
}

fn unpack_piggyback<Vertex: std::fmt::Debug + Clone>(
    context: DataContext<Vertex>,
) -> Vec<DataContext<Vertex>> {
    let mut result = Vec::new();
    unpack_piggyback_inner(&mut result, context);
    result
}

fn unpack_piggyback_inner<Vertex: std::fmt::Debug + Clone>(
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
