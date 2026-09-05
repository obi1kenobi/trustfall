//! Async resolver helpers.
//!
//! The sequential helpers mirror [`super::helpers`]. The concurrent helpers use bounded,
//! order-preserving fan-out for independent I/O. All helpers implement the interpreter's missing
//! optional contract, so adapter code can focus only on present vertices.

use std::{fmt::Debug, future::Future};

use futures_util::{StreamExt as _, future::Either, stream};

use crate::ir::FieldValue;

use super::{
    AsVertex, DataContext,
    async_adapter::{
        AsyncContextOutcomeStream, AsyncContextStream, AsyncNeighborStream, ContextOutcomeStream,
        ContextStream, NeighborOutcomeStream, VertexStream,
    },
};

/// Async mirror of [`resolve_property_with`](super::helpers::resolve_property_with).
///
/// The resolver is called only for present vertices. A missing optional vertex still produces its
/// matching context, with [`FieldValue::Null`] as the property value.
pub fn resolve_property_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
>(
    contexts: AsyncContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> FieldValue + 'vertex,
) -> AsyncContextOutcomeStream<'vertex, V, FieldValue> {
    Box::pin(contexts.map(move |ctx| {
        let value = match ctx.active_vertex::<Vertex>() {
            None => FieldValue::Null,
            Some(vertex) => resolver(vertex),
        };
        (ctx, value)
    }))
}

/// Fallible variant of [`resolve_property_with`].
///
/// A resolver error travels with the context's outcome: the affected row bubbles up to the
/// result stream as a failed row carrying this error, while every other row continues.
/// Missing optional vertices still resolve to `Null` without calling `resolver`.
pub fn try_resolve_property_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
>(
    contexts: ContextStream<'vertex, V, E>,
    mut resolver: impl FnMut(&Vertex) -> Result<FieldValue, E> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, FieldValue, E> {
    Box::pin(contexts.map(move |result| {
        result.and_then(|ctx| {
            let outcome = match ctx.active_vertex::<Vertex>() {
                None => Ok(FieldValue::Null),
                Some(vertex) => resolver(vertex),
            };
            outcome.map(|value| (ctx, value))
        })
    }))
}

/// Async mirror of [`resolve_neighbors_with`](super::helpers::resolve_neighbors_with).
///
/// The resolver is called only for present vertices. A missing optional vertex returns an empty
/// neighbor stream, which preserves the optional scope for later output resolution.
pub fn resolve_neighbors_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
>(
    contexts: AsyncContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> VertexStream<'vertex, Vertex> + 'vertex,
) -> AsyncContextOutcomeStream<'vertex, V, AsyncNeighborStream<'vertex, Vertex>> {
    Box::pin(contexts.map(move |ctx| {
        let neighbors = match ctx.active_vertex::<Vertex>() {
            None => Box::pin(stream::empty()) as AsyncNeighborStream<'vertex, Vertex>,
            Some(vertex) => resolver(vertex),
        };
        (ctx, neighbors)
    }))
}

/// Fallible, context-level variant of [`resolve_neighbors_with`].
///
/// `resolver` either produces the complete neighbor collection for a context or reports that
/// its edge resolution failed. Since this failure happens before neighbor streaming begins, it
/// is returned in the outer context stream.
pub fn try_resolve_neighbors_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
    Neighbors: IntoIterator<Item = Vertex>,
>(
    contexts: ContextStream<'vertex, V, E>,
    mut resolver: impl FnMut(&Vertex) -> Result<Neighbors, E> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, NeighborOutcomeStream<'vertex, Vertex, E>, E>
where
    Neighbors::IntoIter: 'vertex,
{
    Box::pin(contexts.map(move |result| {
        result.and_then(|ctx| match ctx.active_vertex::<Vertex>() {
            None => {
                let no_neighbors: VertexStream<'vertex, Result<Vertex, E>> =
                    Box::pin(stream::empty());
                Ok((ctx, no_neighbors))
            }
            Some(vertex) => resolver(vertex).map(|neighbors| {
                let neighbors: VertexStream<'vertex, Result<Vertex, E>> =
                    Box::pin(stream::iter(neighbors.into_iter().map(Ok)));
                (ctx, neighbors)
            }),
        })
    }))
}

/// Async mirror of [`resolve_coercion_with`](super::helpers::resolve_coercion_with).
///
/// The resolver is called only for present vertices. Missing optional vertices resolve to `false`,
/// which lets the interpreter preserve their enclosing optional scope.
pub fn resolve_coercion_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
>(
    contexts: AsyncContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> bool + 'vertex,
) -> AsyncContextOutcomeStream<'vertex, V, bool> {
    Box::pin(contexts.map(move |ctx| {
        let can_coerce = ctx.active_vertex::<Vertex>().is_some_and(&mut resolver);
        (ctx, can_coerce)
    }))
}

/// Fallible variant of [`resolve_coercion_with`].
///
/// Missing optional vertices resolve to `false` without calling `resolver`.
pub fn try_resolve_coercion_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
>(
    contexts: ContextStream<'vertex, V, E>,
    mut resolver: impl FnMut(&Vertex) -> Result<bool, E> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, bool, E> {
    Box::pin(contexts.map(move |result| {
        result.and_then(|ctx| {
            let outcome = match ctx.active_vertex::<Vertex>() {
                None => Ok(false),
                Some(vertex) => resolver(vertex),
            };
            outcome.map(|can_coerce| (ctx, can_coerce))
        })
    }))
}

/// Order-preserving, bounded concurrent map over a fallible context stream.
///
/// At most `concurrency` futures run at once. Completed work is held until all earlier contexts
/// finish, so output order — including upstream and newly produced errors — matches input order
/// even when the underlying I/O does not. `concurrency` must be at least one.
pub fn map_contexts_buffered<'vertex, V, O, E: 'vertex, F, Fut>(
    contexts: ContextStream<'vertex, V, E>,
    concurrency: usize,
    f: F,
) -> ContextOutcomeStream<'vertex, V, O, E>
where
    V: 'vertex,
    O: 'vertex,
    F: FnMut(DataContext<V>) -> Fut + 'vertex,
    Fut: Future<Output = Result<(DataContext<V>, O), E>> + 'vertex,
{
    assert!(concurrency >= 1, "concurrency must be at least 1");
    let mut f = f;
    Box::pin(
        contexts
            .map(move |result| match result {
                Ok(context) => Either::Left(f(context)),
                Err(error) => Either::Right(std::future::ready(Err(error))),
            })
            .buffered(concurrency),
    )
}

/// Concurrent, order-preserving fallible property resolution.
///
/// The input vertex is cloned before its future starts, which lets the future own its request
/// state. Missing optional vertices bypass the resolver and produce `Null` immediately.
pub fn try_resolve_property_with_concurrent<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
    F,
    Fut,
>(
    contexts: ContextStream<'vertex, V, E>,
    concurrency: usize,
    resolver: F,
) -> ContextOutcomeStream<'vertex, V, FieldValue, E>
where
    F: Fn(Vertex) -> Fut + 'vertex,
    Fut: Future<Output = Result<FieldValue, E>> + 'vertex,
{
    map_contexts_buffered(contexts, concurrency, move |ctx| {
        let pending = ctx.active_vertex::<Vertex>().cloned().map(&resolver);
        async move {
            let outcome = match pending {
                None => Ok(FieldValue::Null),
                Some(fut) => fut.await,
            };
            outcome.map(|value| (ctx, value))
        }
    })
}

/// Concurrent, order-preserving, context-level fallible neighbor resolution.
///
/// A completed resolver future becomes the complete neighbor stream for one context. Missing
/// optional vertices bypass the resolver and produce an empty stream.
pub fn try_resolve_neighbors_with_concurrent<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
    F,
    Fut,
    Neighbors,
>(
    contexts: ContextStream<'vertex, V, E>,
    concurrency: usize,
    resolver: F,
) -> ContextOutcomeStream<'vertex, V, NeighborOutcomeStream<'vertex, Vertex, E>, E>
where
    F: Fn(Vertex) -> Fut + 'vertex,
    Fut: Future<Output = Result<Neighbors, E>> + 'vertex,
    Neighbors: IntoIterator<Item = Vertex>,
    Neighbors::IntoIter: 'vertex,
{
    map_contexts_buffered(contexts, concurrency, move |ctx| {
        let pending = ctx.active_vertex::<Vertex>().cloned().map(&resolver);
        async move {
            let resolution: NeighborOutcomeStream<'vertex, Vertex, E> = match pending {
                None => Box::pin(stream::empty()),
                Some(fut) => {
                    let iter = fut.await?;
                    Box::pin(stream::iter(iter.into_iter().map(Ok)))
                }
            };
            Ok((ctx, resolution))
        }
    })
}

/// Concurrent, order-preserving fallible coercion resolution.
///
/// Missing optional vertices bypass the resolver and produce `false`.
pub fn try_resolve_coercion_with_concurrent<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
    F,
    Fut,
>(
    contexts: ContextStream<'vertex, V, E>,
    concurrency: usize,
    resolver: F,
) -> ContextOutcomeStream<'vertex, V, bool, E>
where
    F: Fn(Vertex) -> Fut + 'vertex,
    Fut: Future<Output = Result<bool, E>> + 'vertex,
{
    map_contexts_buffered(contexts, concurrency, move |ctx| {
        let pending = ctx.active_vertex::<Vertex>().cloned().map(&resolver);
        async move {
            let outcome = match pending {
                None => Ok(false),
                Some(fut) => fut.await,
            };
            outcome.map(|can_coerce| (ctx, can_coerce))
        }
    })
}
