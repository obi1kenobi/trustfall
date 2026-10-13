//! The stream execution kernel's adapter contract.
//!
//! [`FallibleAsyncAdapter`] is what every execution stage is written against. Context streams
//! are fallible end to end: a resolver receives the engine's existing `Result` stream and
//! returns another one, so an earlier failure keeps its position among later rows.
//!
//! This module is private. The public asynchronous adapter API is a separate surface built on
//! top of this contract.

use std::{fmt::Debug, pin::Pin, sync::Arc};

use futures_core::Stream;

use crate::ir::{EdgeParameters, FieldValue};

use super::{AsVertex, DataContext, ResolveEdgeInfo, ResolveInfo};

/// A pinned, boxed [`Stream`] of `T`.
pub type VertexStream<'vertex, T> = Pin<Box<dyn Stream<Item = T> + 'vertex>>;

/// Contexts supplied to a [`FallibleAsyncAdapter`] resolver.
pub type ContextStream<'vertex, V, E = std::convert::Infallible> =
    VertexStream<'vertex, Result<DataContext<V>, E>>;

/// Resolver outcomes produced by a [`FallibleAsyncAdapter`].
pub type ContextOutcomeStream<'vertex, V, O, E = std::convert::Infallible> =
    VertexStream<'vertex, Result<(DataContext<V>, O), E>>;

/// Neighbor vertices produced by a [`FallibleAsyncAdapter`].
pub type NeighborOutcomeStream<'vertex, V, E = std::convert::Infallible> =
    VertexStream<'vertex, Result<V, E>>;

/// An asynchronous data provider that may fail while resolving data.
///
/// Use this trait when an adapter needs to surface a concrete resolver error. Upstream failures
/// remain in the context stream, and each resolver preserves their order.
pub trait FallibleAsyncAdapter<'vertex> {
    /// The type of vertices this adapter queries.
    type Vertex: Clone + Debug + 'vertex;

    /// The error type reported by resolvers.
    type Error: std::error::Error + 'static;

    /// Resolve a starting edge into vertices.
    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>>;

    /// Resolve one property for every input context.
    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, FieldValue, Self::Error>;

    /// Resolve neighboring vertices for every input context.
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeStream<
        'vertex,
        V,
        NeighborOutcomeStream<'vertex, Self::Vertex, Self::Error>,
        Self::Error,
    >;

    /// Test each input vertex for a requested subtype.
    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, bool, Self::Error>;
}
