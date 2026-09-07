//! A concise, infallible async adapter interface.

use std::{fmt::Debug, sync::Arc};

use futures_util::StreamExt as _;

use crate::ir::{EdgeParameters, FieldValue};

use super::{
    AsVertex, ResolveEdgeInfo, ResolveInfo, Typename,
    async_adapter::{
        AsyncAdapter, AsyncContextOutcomeStream, AsyncContextStream, AsyncNeighborStream,
    },
};

/// The smaller implementation surface for an infallible [`AsyncAdapter`].
///
/// It uses `&str` names, handles `__typename`, and omits resolution metadata. Implement
/// [`FallibleAsyncAdapter`](super::FallibleAsyncAdapter) directly when a resolver can fail.
pub trait AsyncBasicAdapter<'vertex> {
    /// The type of vertices this adapter queries.
    type Vertex: Typename + Clone + Debug + 'vertex;

    /// Resolve a schema starting edge.
    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> AsyncNeighborStream<'vertex, Self::Vertex>;

    /// Resolve a property for every input context.
    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &str,
        property_name: &str,
    ) -> AsyncContextOutcomeStream<'vertex, V, FieldValue>;

    /// Resolve an edge for every input context.
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &str,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> AsyncContextOutcomeStream<'vertex, V, AsyncNeighborStream<'vertex, Self::Vertex>>;

    /// Test whether every input vertex has the requested subtype.
    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> AsyncContextOutcomeStream<'vertex, V, bool>;

    /// Resolve `__typename` using [`Typename`].
    fn resolve_typename<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        _type_name: &str,
    ) -> AsyncContextOutcomeStream<'vertex, V, FieldValue> {
        Box::pin(contexts.map(|context| {
            let value = context
                .active_vertex::<Self::Vertex>()
                .map_or(FieldValue::Null, |vertex| vertex.typename().into());
            (context, value)
        }))
    }
}

impl<'vertex, T> AsyncAdapter<'vertex> for T
where
    T: AsyncBasicAdapter<'vertex>,
{
    type Vertex = T::Vertex;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &ResolveInfo,
    ) -> AsyncNeighborStream<'vertex, Self::Vertex> {
        AsyncBasicAdapter::resolve_starting_vertices(self, edge_name, parameters)
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> AsyncContextOutcomeStream<'vertex, V, FieldValue> {
        if property_name.as_ref() == "__typename" {
            self.resolve_typename(contexts, type_name)
        } else {
            AsyncBasicAdapter::resolve_property(self, contexts, type_name, property_name)
        }
    }

    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &ResolveEdgeInfo,
    ) -> AsyncContextOutcomeStream<'vertex, V, AsyncNeighborStream<'vertex, Self::Vertex>> {
        AsyncBasicAdapter::resolve_neighbors(self, contexts, type_name, edge_name, parameters)
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: AsyncContextStream<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &ResolveInfo,
    ) -> AsyncContextOutcomeStream<'vertex, V, bool> {
        AsyncBasicAdapter::resolve_coercion(self, contexts, type_name, coerce_to_type)
    }
}
