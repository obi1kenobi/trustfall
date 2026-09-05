//! Stream-based adapter traits.
//!
//! [`AsyncAdapter`] is the direct asynchronous API: resolvers exchange values and streams, not
//! synthetic `Result`s. Implement [`FallibleAsyncAdapter`] only when resolution can actually
//! fail. The blanket implementation lifts every `AsyncAdapter` into the fallible contract.

use std::{fmt::Debug, pin::Pin, sync::Arc};

use futures_core::Stream;
use futures_util::StreamExt as _;

use crate::ir::{EdgeParameters, FieldValue};

use super::{AsVertex, DataContext, ResolveEdgeInfo, ResolveInfo};

/// A pinned, boxed [`Stream`] of `T`.
pub type VertexStream<'vertex, T> = Pin<Box<dyn Stream<Item = T> + 'vertex>>;

/// Contexts supplied to an [`AsyncAdapter`] resolver.
pub type AsyncContextStream<'vertex, V> = VertexStream<'vertex, DataContext<V>>;

/// Resolver outcomes produced by an [`AsyncAdapter`].
pub type AsyncContextOutcomeStream<'vertex, V, O> = VertexStream<'vertex, (DataContext<V>, O)>;

/// Neighbor vertices produced by an [`AsyncAdapter`].
pub type AsyncNeighborStream<'vertex, V> = VertexStream<'vertex, V>;

/// Fallible contexts supplied to a [`FallibleAsyncAdapter`] resolver.
pub type ContextStream<'vertex, V, E = std::convert::Infallible> =
    VertexStream<'vertex, Result<DataContext<V>, E>>;

/// Fallible resolver outcomes produced by a [`FallibleAsyncAdapter`].
pub type ContextOutcomeStream<'vertex, V, O, E = std::convert::Infallible> =
    VertexStream<'vertex, Result<(DataContext<V>, O), E>>;

/// Neighbor vertices produced by a [`FallibleAsyncAdapter`].
pub type NeighborOutcomeStream<'vertex, V, E = std::convert::Infallible> =
    VertexStream<'vertex, Result<V, E>>;

/// An asynchronous data provider that cannot fail while resolving data.
///
/// Resolver streams are lazy and preserve context order. The trait does not require an executor
/// or `Send`; callers can run it on any runtime that supports local tasks.
pub trait AsyncAdapter<'vertex> {
    /// The type of vertices this adapter queries.
    type Vertex: Clone + Debug + 'vertex;

    /// Resolve a starting edge into vertices.
    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> AsyncNeighborStream<'vertex, Self::Vertex>;

    /// Resolve one property for every input context.
    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> AsyncContextOutcomeStream<'vertex, V, FieldValue>;

    /// Resolve neighboring vertices for every input context.
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> AsyncContextOutcomeStream<'vertex, V, AsyncNeighborStream<'vertex, Self::Vertex>>;

    /// Test each input vertex for a requested subtype.
    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> AsyncContextOutcomeStream<'vertex, V, bool>;
}

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

impl<'vertex, T> FallibleAsyncAdapter<'vertex> for T
where
    T: AsyncAdapter<'vertex>,
{
    type Vertex = T::Vertex;
    type Error = std::convert::Infallible;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>> {
        Box::pin(
            AsyncAdapter::resolve_starting_vertices(self, edge_name, parameters, resolve_info)
                .map(Ok),
        )
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, FieldValue, Self::Error> {
        let contexts = Box::pin(
            contexts.map(|context| context.expect("infallible adapter received an error")),
        );
        Box::pin(
            AsyncAdapter::resolve_property(self, contexts, type_name, property_name, resolve_info)
                .map(Ok),
        )
    }

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
    > {
        let contexts = Box::pin(
            contexts.map(|context| context.expect("infallible adapter received an error")),
        );
        Box::pin(
            AsyncAdapter::resolve_neighbors(
                self,
                contexts,
                type_name,
                edge_name,
                parameters,
                resolve_info,
            )
            .map(|(context, neighbors)| {
                (
                    context,
                    Box::pin(neighbors.map(Ok))
                        as NeighborOutcomeStream<'vertex, Self::Vertex, Self::Error>,
                )
            })
            .map(Ok),
        )
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V, Self::Error>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, bool, Self::Error> {
        let contexts = Box::pin(
            contexts.map(|context| context.expect("infallible adapter received an error")),
        );
        Box::pin(
            AsyncAdapter::resolve_coercion(self, contexts, type_name, coerce_to_type, resolve_info)
                .map(Ok),
        )
    }
}
