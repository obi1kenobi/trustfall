//! Async resolver helpers.
//!
//! The sequential helpers mirror [`super::helpers`]. The concurrent helpers use bounded,
//! order-preserving fan-out for independent I/O. All helpers implement the interpreter's missing
//! optional contract, so adapter code can focus only on present vertices.

use std::{fmt::Debug, future::Future};

use futures_util::{StreamExt as _, stream};

use crate::ir::FieldValue;

use super::{
    AsVertex, DataContext,
    async_adapter::{ContextOutcomeStream, ContextStream, NeighborResolutionStream, VertexStream},
};

/// Async mirror of [`resolve_property_with`](super::helpers::resolve_property_with).
///
/// The resolver is called only for present vertices. A missing optional vertex still produces its
/// matching context, with [`FieldValue::Null`] as the property value.
pub fn resolve_property_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> FieldValue + 'vertex,
) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, E>> {
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => (ctx, Ok(FieldValue::Null)),
        Some(vertex) => {
            let value = resolver(vertex);
            (ctx, Ok(value))
        }
    }))
}

/// Fallible variant of [`resolve_property_with`].
///
/// Errors apply to the current context and stop the query result stream when the interpreter sees
/// them. Missing optional vertices still resolve to `Null` without calling `resolver`.
pub fn try_resolve_property_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> Result<FieldValue, E> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, E>> {
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => (ctx, Ok(FieldValue::Null)),
        Some(vertex) => {
            let outcome = resolver(vertex);
            (ctx, outcome)
        }
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
    E: 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> VertexStream<'vertex, Vertex> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, NeighborResolutionStream<'vertex, Vertex, E>> {
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => {
            let no_neighbors: VertexStream<'vertex, Result<Vertex, E>> = Box::pin(stream::empty());
            (ctx, Ok(no_neighbors))
        }
        Some(vertex) => {
            let neighbors = resolver(vertex);
            let neighbors: VertexStream<'vertex, Result<Vertex, E>> = Box::pin(neighbors.map(Ok));
            (ctx, Ok(neighbors))
        }
    }))
}

/// Fallible, context-level variant of [`resolve_neighbors_with`].
///
/// `resolver` either produces the complete neighbor collection for a context or reports that its
/// edge resolution failed. Use a custom [`NeighborResolutionStream`] when failures can arise while
/// producing individual neighbors instead.
pub fn try_resolve_neighbors_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
    Neighbors: IntoIterator<Item = Vertex>,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> Result<Neighbors, E> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, NeighborResolutionStream<'vertex, Vertex, E>>
where
    Neighbors::IntoIter: 'vertex,
{
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => {
            let no_neighbors: VertexStream<'vertex, Result<Vertex, E>> = Box::pin(stream::empty());
            (ctx, Ok(no_neighbors))
        }
        Some(vertex) => {
            let outcome = resolver(vertex).map(|neighbors| {
                let neighbors: VertexStream<'vertex, Result<Vertex, E>> =
                    Box::pin(stream::iter(neighbors.into_iter().map(Ok)));
                neighbors
            });
            (ctx, outcome)
        }
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
    E: 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> bool + 'vertex,
) -> ContextOutcomeStream<'vertex, V, Result<bool, E>> {
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => (ctx, Ok(false)),
        Some(vertex) => {
            let can_coerce = resolver(vertex);
            (ctx, Ok(can_coerce))
        }
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
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> Result<bool, E> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, Result<bool, E>> {
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => (ctx, Ok(false)),
        Some(vertex) => {
            let outcome = resolver(vertex);
            (ctx, outcome)
        }
    }))
}

/// Order-preserving, bounded concurrent map over a context stream.
///
/// At most `concurrency` futures run at once. Completed work is held until all earlier contexts
/// finish, so output order matches input order even when the underlying I/O does not. `concurrency`
/// must be at least one.
pub fn map_contexts_buffered<'vertex, V, O, F, Fut>(
    contexts: ContextStream<'vertex, V>,
    concurrency: usize,
    f: F,
) -> ContextOutcomeStream<'vertex, V, O>
where
    V: 'vertex,
    O: 'vertex,
    F: FnMut(DataContext<V>) -> Fut + 'vertex,
    Fut: Future<Output = (DataContext<V>, O)> + 'vertex,
{
    assert!(concurrency >= 1, "concurrency must be at least 1");
    Box::pin(contexts.map(f).buffered(concurrency))
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
    contexts: ContextStream<'vertex, V>,
    concurrency: usize,
    resolver: F,
) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, E>>
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
            (ctx, outcome)
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
    contexts: ContextStream<'vertex, V>,
    concurrency: usize,
    resolver: F,
) -> ContextOutcomeStream<'vertex, V, NeighborResolutionStream<'vertex, Vertex, E>>
where
    F: Fn(Vertex) -> Fut + 'vertex,
    Fut: Future<Output = Result<Neighbors, E>> + 'vertex,
    Neighbors: IntoIterator<Item = Vertex>,
    Neighbors::IntoIter: 'vertex,
{
    map_contexts_buffered(contexts, concurrency, move |ctx| {
        let pending = ctx.active_vertex::<Vertex>().cloned().map(&resolver);
        async move {
            let resolution: NeighborResolutionStream<'vertex, Vertex, E> = match pending {
                None => Ok(Box::pin(stream::empty())),
                Some(fut) => match fut.await {
                    Ok(iter) => Ok(Box::pin(stream::iter(iter.into_iter().map(Ok)))),
                    Err(e) => Err(e),
                },
            };
            (ctx, resolution)
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
    contexts: ContextStream<'vertex, V>,
    concurrency: usize,
    resolver: F,
) -> ContextOutcomeStream<'vertex, V, Result<bool, E>>
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
            (ctx, outcome)
        }
    })
}
