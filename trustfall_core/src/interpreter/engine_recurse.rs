//! Stream-based `@recurse` edge expansion.
//!
//! This is the asynchronous form of [`expand_recursive_edge`](super::execution). A recursive edge
//! returns every reachable depth, not just the deepest one. The stream therefore carries parent
//! contexts beside the first child that continues recursion, then unpacks those saved contexts at
//! the end.

use std::sync::Arc;

use async_stream::stream;
use futures_util::StreamExt;

use crate::ir::Recursive;

use super::{
    DataContext,
    async_adapter::AsyncAdapter,
    engine::{EdgeRef, FallibleContextStream},
    execution::QueryCarrier,
};

/// Expand a recursive edge before entering its destination vertex.
///
/// Contexts that cannot continue at one depth are suspended instead of discarded: they still
/// represent a result at an earlier depth. Once every expansion is complete, suspension and
/// piggyback state are removed before the destination vertex is processed.
pub(super) fn expand_recursive_edge<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: Arc<AdapterT>,
    carrier: &mut QueryCarrier,
    edge: &EdgeRef<'_>,
    recursive: &Recursive,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    let expanding_from_vid = edge.from.vid;

    // A missing optional has no neighbors, but it still needs the same suspend/resume balancing
    // as a real vertex. The `None` sentinel lets the final `ensure_unsuspended()` restore it.
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

    let edge_endpoint_type = edge.to.coerced_from_type.as_ref().unwrap_or(&edge.to.type_name);
    let recursing_from = recursive.coerce_to.as_ref().unwrap_or(edge_endpoint_type);

    // The first hop starts from the edge source. Later hops start from the previous destination,
    // which may use a different type after an explicit recursion coercion.
    recursion_stream = perform_one_recursive_edge_expansion(
        adapter.as_ref(),
        carrier,
        edge,
        &edge.from.type_name,
        recursion_stream,
    );

    for _ in 2..=max_depth {
        if let Some(coerce_to) = recursive.coerce_to.as_ref() {
            // A vertex that fails this coercion is still an answer for a shallower depth. Suspend
            // it so the next edge resolver sees no active vertex and produces no further children.
            let coercion_outcomes = carrier.resolve_with(expanding_from_vid, false, |info| {
                adapter.resolve_coercion(recursion_stream, edge_endpoint_type, coerce_to, info)
            });

            recursion_stream = Box::pin(coercion_outcomes.map(|result| {
                result.map(
                    |(context, can_coerce)| {
                        if can_coerce { context } else { context.ensure_suspended() }
                    },
                )
            }));
        }

        recursion_stream = perform_one_recursive_edge_expansion(
            adapter.as_ref(),
            carrier,
            edge,
            recursing_from,
            recursion_stream,
        );
    }

    // A piggyback holds contexts that must be emitted before the descendant which carried them.
    // Flatten lazily, restoring each suspended vertex as it leaves recursion. An error stays a
    // one-element stream at its original position.
    Box::pin(recursion_stream.flat_map(|result| {
        stream! {
            match result {
                Ok(context) => {
                    for context in unpack_piggyback(context) {
                        assert!(context.piggyback.is_none());
                        yield Ok(context.ensure_unsuspended());
                    }
                }
                Err(error) => yield Err(error),
            }
        }
    }))
}

fn perform_one_recursive_edge_expansion<'query, AdapterT: AsyncAdapter<'query> + 'query>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    edge: &EdgeRef<'_>,
    expanding_from_type: &Arc<str>,
    stream: FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error>,
) -> FallibleContextStream<'query, AdapterT::Vertex, AdapterT::Error> {
    // The initial context is at the edge source; later iterations are at the prior neighbor. In
    // either case the active vertex is already correct, unlike ordinary edge expansion.
    let edge_outcomes = carrier.resolve_edge_with(edge.from.vid, edge.to.vid, edge.eid, |info| {
        adapter.resolve_neighbors(stream, expanding_from_type, edge.name, edge.parameters, info)
    });

    Box::pin(stream! {
        let mut edge_outcomes = edge_outcomes;
        while let Some(item) = edge_outcomes.next().await {
            // A failed row slot from upstream passes through untouched.
            let (context, neighbors) = match item {
                Ok(pair) => pair,
                Err(error) => {
                    yield Err(error);
                    continue;
                }
            };
            let mut neighbors = neighbors;

            // Move the parent exactly once into the first child. Its vertex-less sibling becomes
            // the template for every additional neighbor of this same parent.
            let mut context_slot: Option<DataContext<AdapterT::Vertex>> = Some(context);
            let mut neighbor_base: Option<DataContext<AdapterT::Vertex>> = None;

            while let Some(neighbor) = neighbors.next().await {
                let vertex = match neighbor {
                    Ok(vertex) => vertex,
                    // A failed neighbor becomes a failed row of its own, in its position.
                    Err(error) => {
                        yield Err(error);
                        continue;
                    }
                };

                if let Some(ctx) = context_slot.take() {
                    // The parent is an answer at the current depth. Attach it to the first child
                    // rather than yield it now, so all deeper descendants remain adjacent to it.
                    let base = ctx.split_and_move_to_vertex(None);
                    let mut neighbor_ctx = ctx.split_and_move_to_vertex(Some(vertex));
                    neighbor_ctx
                        .piggyback
                        .get_or_insert_with(Default::default)
                        .push(ctx.ensure_suspended());
                    neighbor_base = Some(base);
                    yield Ok(neighbor_ctx);
                } else {
                    yield Ok(neighbor_base
                        .as_ref()
                        .unwrap()
                        .split_and_move_to_vertex(Some(vertex)));
                }
            }

            if let Some(ctx) = context_slot {
                // No child was produced. This context is still an answer at its current depth;
                // the final unpacking step will restore any suspended vertex.
                yield Ok(ctx);
            }
        }
    })
}

/// Iterate through a recursive context and its attached parents in result order.
///
/// The explicit stack avoids recursively materializing every row before downstream stages poll
/// for the first one. Pushing siblings in reverse preserves their original left-to-right order.
fn unpack_piggyback<Vertex>(
    context: DataContext<Vertex>,
) -> impl Iterator<Item = DataContext<Vertex>> {
    let mut pending = vec![context];
    std::iter::from_fn(move || {
        loop {
            let mut context = pending.pop()?;
            if let Some(piggyback) = context.piggyback.take() {
                pending.push(context);
                pending.extend(piggyback.into_iter().rev());
            } else {
                return Some(context);
            }
        }
    })
}
