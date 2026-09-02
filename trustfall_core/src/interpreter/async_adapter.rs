//! Asynchronous, `Stream`-native adapter trait and pipeline type aliases.
//!
//! This is the async counterpart of the synchronous [`Adapter`](super::Adapter) trait. Where the
//! synchronous protocol uses lazy [`Iterator`]s, this protocol uses lazy [`Stream`]s, so adapter
//! resolvers can overlap IO across the contexts in a batch (order-preserving concurrency, e.g.
//! `contexts.map(fetch).buffered(N)`), instead of blocking one context at a time.
//!
//! # Error handling is strongly typed and native
//!
//! The execution kernel threads `Result` natively through its streams and fails fast on the first
//! `Err` via `?`. Accordingly, resolver *outputs* carry `Result`s in the outcome slot, exactly as
//! the sync [`Adapter`](super::Adapter) trait does:
//! - `resolve_starting_vertices` yields `Result<Vertex, Error>`,
//! - `resolve_property` / `resolve_coercion` carry `Result<_, Error>` in the outcome,
//! - `resolve_neighbors` outcomes are [`Result`]s of neighbor streams (a context-level failure),
//!   whose individual neighbors may themselves be `Result<Vertex, Error>`.
//!
//! Resolver *inputs* (`contexts`) are plain `DataContext` streams: the engine handles upstream
//! errors around the adapter, so adapters only ever produce errors, never have to forward them.
//!
//! # Runtime-agnostic
//!
//! Nothing here spawns, sleeps, or blocks; the only dependency is `futures` (`Stream`). The engine
//! never requires `Send`, so `!Send` adapters (e.g. WASM) are supported. Streams produced by this
//! engine are `!Send` (the kernel is single-task by construction); callers running on multi-thread
//! executors should drive them from a `LocalSet` or `spawn_local` task and forward the (plain-data)
//! rows across their chosen `Send` boundary.

use std::{fmt::Debug, pin::Pin, sync::Arc};

use futures_core::Stream;

use crate::ir::{EdgeParameters, FieldValue};

use super::{AsVertex, DataContext, ResolveEdgeInfo, ResolveInfo};

/// A pinned, boxed [`Stream`] of `T` — the async counterpart of
/// [`VertexIterator`](super::VertexIterator).
pub type VertexStream<'vertex, VertexT> = Pin<Box<dyn Stream<Item = VertexT> + 'vertex>>;

/// A stream of query contexts flowing into an adapter resolver — the async counterpart of
/// [`ContextIterator`](super::ContextIterator). These are plain contexts: the engine strips and
/// re-surfaces upstream errors around the adapter, so resolvers never see `Err` on their input.
pub type ContextStream<'vertex, VertexT> = VertexStream<'vertex, DataContext<VertexT>>;

/// A stream of `(context, outcome)` pairs — the async counterpart of
/// [`ContextOutcomeIterator`](super::ContextOutcomeIterator).
pub type ContextOutcomeStream<'vertex, VertexT, OutcomeT> =
    Pin<Box<dyn Stream<Item = (DataContext<VertexT>, OutcomeT)> + 'vertex>>;

/// The outcome of resolving an edge for a single context — the async counterpart of
/// [`NeighborResolution`](super::NeighborResolution): either the lazily-produced stream of
/// (individually fallible) neighboring vertices, or an error that failed the edge resolution
/// for that context as a whole.
pub type NeighborResolutionStream<'vertex, VertexT, ErrorT> =
    Result<VertexStream<'vertex, Result<VertexT, ErrorT>>, ErrorT>;

/// Asynchronous data providers implement this trait to enable streaming query execution.
///
/// It mirrors [`FallibleAdapter`](super::FallibleAdapter) method-for-method; see that trait for the detailed
/// preconditions and postconditions of each resolver (they are identical here). The differences
/// are purely in shape: inputs and outputs are [`Stream`]s rather than [`Iterator`]s.
///
/// For most adapters, [`AsyncBasicAdapter`](super::async_basic_adapter::AsyncBasicAdapter) is the
/// smaller implementation surface and provides this trait through a blanket implementation.
pub trait AsyncAdapter<'vertex> {
    /// The type of vertices in the dataset this adapter queries. See [`FallibleAdapter::Vertex`].
    ///
    /// [`FallibleAdapter::Vertex`]: super::FallibleAdapter::Vertex
    type Vertex: Clone + Debug + 'vertex;

    /// The error type this adapter may report. See [`FallibleAdapter::Error`].
    ///
    /// [`FallibleAdapter::Error`]: super::FallibleAdapter::Error
    type Error: std::error::Error + 'static;

    /// Produce a stream of vertices for the specified starting edge.
    /// See [`Adapter::resolve_starting_vertices`](super::Adapter::resolve_starting_vertices).
    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>>;

    /// Resolve a property required by the query that's being evaluated.
    /// See [`Adapter::resolve_property`](super::Adapter::resolve_property).
    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, Self::Error>>;

    /// Resolve the neighboring vertices across an edge.
    ///
    /// See [`Adapter::resolve_neighbors`](super::Adapter::resolve_neighbors) for the shared
    /// preconditions and postconditions. The outcome for each context is either the stream of
    /// (individually fallible) neighbors, or an error that failed resolving that context's edge
    /// as a whole — for example, a failed batched fetch for that context.
    #[allow(clippy::type_complexity)]
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<
        'vertex,
        V,
        NeighborResolutionStream<'vertex, Self::Vertex, Self::Error>,
    >;

    /// Attempt to coerce vertices to a subtype, as required by the query that's being evaluated.
    /// See [`Adapter::resolve_coercion`](super::Adapter::resolve_coercion).
    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<bool, Self::Error>>;
}
