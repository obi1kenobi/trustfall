//! A simplified async adapter trait and its [`AsyncAdapter`] implementation.
//!
//! This is the async counterpart of [`BasicAdapter`](super::basic_adapter::BasicAdapter). It uses
//! `&str` names, omits resolver hints, and resolves `__typename` through [`Typename`]. Implement
//! it when those conveniences are sufficient; use [`AsyncAdapter`] directly to inspect hints or
//! report an edge-resolution error before producing its neighbor stream.
//!
//! The blanket implementation supplies [`AsyncAdapter`]. Infallible adapters set `Error` to
//! [`std::convert::Infallible`].

use std::{fmt::Debug, sync::Arc};

use crate::ir::{EdgeParameters, FieldValue};

use futures_util::StreamExt as _;

use super::{
    AsVertex, Typename,
    async_adapter::{
        AsyncAdapter, ContextOutcomeStream, ContextStream, NeighborResolutionStream, VertexStream,
    },
};

/// A smaller [`AsyncAdapter`] interface for most async adapters.
///
/// It uses `&str` names, omits resolver hints, and resolves `__typename` automatically.
/// Implementing this trait also implements [`AsyncAdapter`]. Its methods still obey the full
/// stream contract: one outcome per context, in input order.
pub trait AsyncBasicAdapter<'vertex> {
    /// The type of vertices this adapter queries.
    ///
    /// An `Arc<_>` vertex can make cloning cheap.
    type Vertex: Typename + Clone + Debug + 'vertex;

    /// The error type this adapter may report.
    type Error: std::error::Error + 'static;

    /// Resolve a schema starting edge.
    ///
    /// Each item becomes one root query context. Returning an error stops query execution.
    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>>;

    /// Resolve a property for every context.
    ///
    /// Return one result per context, in input order. A context without an active vertex must
    /// resolve to `Ok(FieldValue::Null)`. `__typename` is handled by [`Self::resolve_typename`].
    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, Self::Error>>;

    /// Resolve an edge for every context.
    ///
    /// Return one result per context, in input order. A context without an active vertex must
    /// have an empty neighbor stream. Successful neighbors must match the schema's edge type.
    #[allow(clippy::type_complexity)]
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &str,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> ContextOutcomeStream<'vertex, V, VertexStream<'vertex, Result<Self::Vertex, Self::Error>>>;

    /// Test whether each context's active vertex has the requested subtype.
    ///
    /// Return one result per context, in input order. A context without an active vertex must
    /// resolve to `Ok(false)`.
    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeStream<'vertex, V, Result<bool, Self::Error>>;

    /// Resolve `__typename` for every context.
    ///
    /// The default implementation uses [`Typename`]. It returns `Null` for a missing optional
    /// vertex and may be overridden for a more efficient implementation.
    fn resolve_typename<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        _type_name: &str,
    ) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, Self::Error>> {
        Box::pin(contexts.map(|ctx| match ctx.active_vertex::<Self::Vertex>() {
            None => (ctx, Ok(FieldValue::Null)),
            Some(vertex) => {
                let value: FieldValue = vertex.typename().into();
                (ctx, Ok(value))
            }
        }))
    }
}

impl<'vertex, T> AsyncAdapter<'vertex> for T
where
    T: AsyncBasicAdapter<'vertex>,
{
    type Vertex = T::Vertex;
    type Error = T::Error;

    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &super::ResolveInfo,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>> {
        <Self as AsyncBasicAdapter>::resolve_starting_vertices(self, edge_name.as_ref(), parameters)
    }

    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        _resolve_info: &super::ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, Self::Error>> {
        if property_name.as_ref() == "__typename" {
            self.resolve_typename(contexts, type_name.as_ref())
        } else {
            <Self as AsyncBasicAdapter>::resolve_property(
                self,
                contexts,
                type_name.as_ref(),
                property_name.as_ref(),
            )
        }
    }

    #[allow(clippy::type_complexity)]
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        _resolve_info: &super::ResolveEdgeInfo,
    ) -> ContextOutcomeStream<
        'vertex,
        V,
        NeighborResolutionStream<'vertex, Self::Vertex, Self::Error>,
    > {
        // The basic trait has no context-level edge error. Wrap its neighbor stream in `Ok`; any
        // failure it can report already appears as an item in that stream.
        Box::pin(
            <Self as AsyncBasicAdapter>::resolve_neighbors(
                self,
                contexts,
                type_name.as_ref(),
                edge_name.as_ref(),
                parameters,
            )
            .map(|(ctx, neighbors)| (ctx, Ok(neighbors))),
        )
    }

    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        _resolve_info: &super::ResolveInfo,
    ) -> ContextOutcomeStream<'vertex, V, Result<bool, Self::Error>> {
        <Self as AsyncBasicAdapter>::resolve_coercion(
            self,
            contexts,
            type_name.as_ref(),
            coerce_to_type.as_ref(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, convert::Infallible, sync::Arc};

    use futures_util::{StreamExt as _, stream};

    use crate::{
        frontend,
        interpreter::{AsVertex, Typename, engine::interpret_ir},
        ir::{EdgeParameters, FieldValue},
        schema::Schema,
    };

    use super::{AsyncBasicAdapter, ContextOutcomeStream, ContextStream, VertexStream};

    #[derive(Clone, Debug)]
    struct Vertex(u8);

    impl Typename for Vertex {
        fn typename(&self) -> &'static str {
            "Item"
        }
    }

    struct NativeAsyncAdapter;

    impl<'a> AsyncBasicAdapter<'a> for NativeAsyncAdapter {
        type Vertex = Vertex;
        type Error = Infallible;

        fn resolve_starting_vertices(
            &self,
            _: &str,
            _: &EdgeParameters,
        ) -> VertexStream<'a, Result<Self::Vertex, Self::Error>> {
            Box::pin(stream::iter((0..4).map(|value| Ok(Vertex(value)))))
        }

        fn resolve_property<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            contexts: ContextStream<'a, V>,
            _: &str,
            _: &str,
        ) -> ContextOutcomeStream<'a, V, Result<FieldValue, Self::Error>> {
            Box::pin(contexts.map(|context| {
                let value = context.active_vertex::<Vertex>().unwrap().0.into();
                (context, Ok(FieldValue::Int64(value)))
            }))
        }

        fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            _: ContextStream<'a, V>,
            _: &str,
            _: &str,
            _: &EdgeParameters,
        ) -> ContextOutcomeStream<'a, V, VertexStream<'a, Result<Self::Vertex, Self::Error>>>
        {
            unreachable!()
        }

        fn resolve_coercion<V: AsVertex<Self::Vertex> + 'a>(
            &self,
            _: ContextStream<'a, V>,
            _: &str,
            _: &str,
        ) -> ContextOutcomeStream<'a, V, Result<bool, Self::Error>> {
            unreachable!()
        }
    }

    #[test]
    fn blanket_adapter_executes_on_the_shared_kernel() {
        let schema = Schema::parse(
            "schema { query: RootSchemaQuery }\n\
             type RootSchemaQuery { Item: [Item!]! }\n\
             type Item { value: Int! }",
        )
        .unwrap();
        let query = frontend::parse(&schema, "{ Item { value @output } }").unwrap();
        let rows =
            interpret_ir(Arc::new(NativeAsyncAdapter), query, Arc::new(BTreeMap::new())).unwrap();

        let rows = futures_executor::block_on(rows.collect::<Vec<_>>());
        assert_eq!(rows.len(), 4);
        assert!(rows.into_iter().all(|row| row.is_ok()));
    }
}
