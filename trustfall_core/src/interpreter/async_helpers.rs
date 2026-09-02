//! Async mirrors of the sync helpers in [`super::helpers`], plus concurrency utilities.
//!
//! # Sequential helpers (default)
//!
//! The sequential helpers apply per-vertex resolver closures over a [`ContextStream`] using
//! [`StreamExt::map`]. No concurrency or buffering is introduced — the closures are applied
//! lazily, one context at a time, mirroring the sync helpers' per-context logic exactly
//! (including the `None` active-vertex cases).
//!
//! # Concurrent helpers (bounded fan-out)
//!
//! For adapters that perform real IO, the `*_concurrent` helpers and [`map_contexts_buffered`]
//! start up to `concurrency` resolver futures at once while **preserving input order** of
//! `(context, outcome)` pairs (via [`StreamExt::buffered`]). Prefer these when each resolution
//! is an independent network/disk call.
//!
//! For the sync counterparts see [`helpers::resolve_property_with`][sync-prop],
//! [`helpers::resolve_neighbors_with`][sync-neigh], and
//! [`helpers::resolve_coercion_with`][sync-coerce].
//!
//! [sync-prop]: super::helpers::resolve_property_with
//! [sync-neigh]: super::helpers::resolve_neighbors_with
//! [sync-coerce]: super::helpers::resolve_coercion_with

use std::{fmt::Debug, future::Future};

use futures_util::{StreamExt as _, stream};

use crate::ir::FieldValue;

use super::{
    AsVertex, DataContext,
    async_adapter::{ContextOutcomeStream, ContextStream, VertexStream},
};

// ---------------------------------------------------------------------------
// Sequential helpers
// ---------------------------------------------------------------------------

/// Async mirror of [`resolve_property_with`](super::helpers::resolve_property_with).
///
/// Applies a **sync** closure `resolver: FnMut(&Vertex) -> FieldValue` over each context in the
/// input stream, one at a time. The result is wrapped in `Ok`.
///
/// When a context's active vertex is `None`, the property outcome is `Ok(FieldValue::Null)`.
pub fn resolve_property_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> FieldValue + 'vertex,
) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, std::convert::Infallible>> {
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
/// Like [`resolve_property_with`] but the resolver closure may return a `Result<FieldValue, E>`.
/// An `Err` is forwarded directly as the context's outcome.
///
/// When a context's active vertex is `None`, the property outcome is `Ok(FieldValue::Null)`.
pub fn try_resolve_property_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E,
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
/// Applies a **sync** closure `resolver: FnMut(&Vertex) -> VertexStream<'vertex, Vertex>` over
/// each context in the input stream, one at a time. Each produced neighbor is wrapped in `Ok`.
///
/// When a context's active vertex is `None`, the neighbors outcome is an empty stream.
pub fn resolve_neighbors_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> VertexStream<'vertex, Vertex> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, VertexStream<'vertex, Result<Vertex, std::convert::Infallible>>>
{
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => {
            let no_neighbors: VertexStream<'vertex, Result<Vertex, std::convert::Infallible>> =
                Box::pin(stream::empty());
            (ctx, no_neighbors)
        }
        Some(vertex) => {
            let neighbors = resolver(vertex);
            let neighbors: VertexStream<'vertex, Result<Vertex, std::convert::Infallible>> =
                Box::pin(neighbors.map(Ok));
            (ctx, neighbors)
        }
    }))
}

/// Fallible variant of [`resolve_neighbors_with`].
///
/// The resolver returns a **stream of per-neighbor results**
/// (`VertexStream<'vertex, Result<Vertex, E>>`), matching
/// [`AsyncAdapter::resolve_neighbors`](super::async_adapter::AsyncAdapter::resolve_neighbors):
///
/// - Each item is a neighbor (`Ok`) or a failure for that neighbor fetch (`Err`).
/// - A context-level failure (cannot resolve the edge at all) is expressed by yielding a single
///   `Err` item, e.g. `stream::once(async { Err(e) })`. The outer outcome slot is always a
///   neighbor **stream**, never `Result<VertexStream, E>` — the trait cannot carry that shape.
/// - When a context's active vertex is `None`, pass an **empty** stream (not an error).
pub fn try_resolve_neighbors_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> VertexStream<'vertex, Result<Vertex, E>> + 'vertex,
) -> ContextOutcomeStream<'vertex, V, VertexStream<'vertex, Result<Vertex, E>>> {
    Box::pin(contexts.map(move |ctx| match ctx.active_vertex::<Vertex>() {
        None => {
            let no_neighbors: VertexStream<'vertex, Result<Vertex, E>> = Box::pin(stream::empty());
            (ctx, no_neighbors)
        }
        Some(vertex) => {
            let neighbors = resolver(vertex);
            (ctx, neighbors)
        }
    }))
}

/// Async mirror of [`resolve_coercion_with`](super::helpers::resolve_coercion_with).
///
/// Applies a **sync** closure `resolver: FnMut(&Vertex) -> bool` over each context in the
/// input stream, one at a time. The result is wrapped in `Ok`.
///
/// When a context's active vertex is `None`, the coercion outcome is `Ok(false)`.
pub fn resolve_coercion_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
>(
    contexts: ContextStream<'vertex, V>,
    mut resolver: impl FnMut(&Vertex) -> bool + 'vertex,
) -> ContextOutcomeStream<'vertex, V, Result<bool, std::convert::Infallible>> {
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
/// Like [`resolve_coercion_with`] but the resolver closure may return `Result<bool, E>`.
/// An `Err` is forwarded directly as the context's outcome.
///
/// When a context's active vertex is `None`, the coercion outcome is `Ok(false)`.
pub fn try_resolve_coercion_with<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E,
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

// ---------------------------------------------------------------------------
// Bounded concurrent / fan-out patterns
// ---------------------------------------------------------------------------

/// Order-preserving, bounded concurrent map over a context stream.
///
/// This is the primitive behind the `*_concurrent` helpers and the recommended **adapter-side
/// fan-out pattern** when implementing [`AsyncAdapter`](super::async_adapter::AsyncAdapter)
/// resolvers by hand:
///
/// ```ignore
/// // Overlap up to 8 independent fetches; outcomes stay in input order.
/// map_contexts_buffered(contexts, 8, |ctx| async move {
///     let value = match ctx.active_vertex::<MyVertex>() {
///         None => Ok(FieldValue::Null),
///         Some(v) => fetch_property(v).await,
///     };
///     (ctx, value)
/// })
/// ```
///
/// # Concurrency model
///
/// - Up to `concurrency` futures produced by `f` may be polled concurrently.
/// - `(context, outcome)` pairs are yielded in the **same order** as the input contexts
///   (uses [`StreamExt::buffered`], not `buffer_unordered`).
/// - `concurrency` must be at least `1` (sequential when `1`).
///
/// # Adapter contract
///
/// Callers must still produce **exactly one outcome per input context**, in order. Concurrency
/// does not relax the 1:1 pairing required by the engine.
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
/// Like [`try_resolve_property_with`], but starts up to `concurrency` async property fetches at
/// once. The `resolver` is invoked with a **cloned** active vertex and returns a future.
///
/// When a context's active vertex is `None`, the outcome is `Ok(FieldValue::Null)` without
/// calling `resolver`.
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

/// Concurrent, order-preserving fallible neighbor resolution.
///
/// The `resolver` is invoked with a **cloned** active vertex and returns a future that resolves
/// to either:
/// - `Ok(neighbors)` — an iterable of neighbor vertices (each becomes `Ok` in the neighbor stream),
/// - `Err(e)` — a **context-level** failure, emitted as a single-item neighbor stream `Err(e)`.
///
/// When a context's active vertex is `None`, the neighbors outcome is an empty stream.
///
/// Outcomes (the outer `(context, neighbor_stream)` pairs) are produced in input order with up
/// to `concurrency` neighbor fetches in flight.
pub fn try_resolve_neighbors_with_concurrent<
    'vertex,
    Vertex: Debug + Clone + 'vertex,
    V: AsVertex<Vertex> + 'vertex,
    E: 'vertex,
    F,
    Fut,
    I,
>(
    contexts: ContextStream<'vertex, V>,
    concurrency: usize,
    resolver: F,
) -> ContextOutcomeStream<'vertex, V, VertexStream<'vertex, Result<Vertex, E>>>
where
    F: Fn(Vertex) -> Fut + 'vertex,
    Fut: Future<Output = Result<I, E>> + 'vertex,
    I: IntoIterator<Item = Vertex> + 'vertex,
{
    map_contexts_buffered(contexts, concurrency, move |ctx| {
        let pending = ctx.active_vertex::<Vertex>().cloned().map(&resolver);
        async move {
            let neighbors: VertexStream<'vertex, Result<Vertex, E>> = match pending {
                None => Box::pin(stream::empty()),
                Some(fut) => match fut.await {
                    Ok(iter) => Box::pin(stream::iter(iter.into_iter().map(Ok))),
                    // Context-level failure: one Err neighbor item (trait cannot carry
                    // Result<VertexStream, E> on the outer outcome).
                    Err(e) => Box::pin(stream::once(async move { Err(e) })),
                },
            };
            (ctx, neighbors)
        }
    })
}

/// Concurrent, order-preserving fallible coercion resolution.
///
/// Like [`try_resolve_coercion_with`], but starts up to `concurrency` async checks at once.
/// When a context's active vertex is `None`, the outcome is `Ok(false)`.
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
