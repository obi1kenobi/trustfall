//! Ergonomic, simplified async adapter trait and its blanket [`AsyncAdapter`] implementation.
//!
//! This is the async counterpart of [`BasicAdapter`](super::basic_adapter::BasicAdapter). It
//! trades a little of [`AsyncAdapter`]'s flexibility for a significantly simpler implementation
//! surface:
//!
//! - `&str` instead of `&Arc<str>` for all names of types, properties, and edges.
//! - Simplified function signatures, with only the minimum necessary arguments.
//! - Automatic handling of the `__typename` special property via [`Typename`].
//!
//! Implementing `AsyncBasicAdapter` gives a "free" [`AsyncAdapter`] implementation through the
//! blanket `impl` at the bottom of this module. Unlike the sync [`BasicAdapter`], which is
//! always infallible, `AsyncBasicAdapter` keeps its own `type Error` because async adapters
//! typically perform IO and must be able to report errors.

use std::{fmt::Debug, sync::Arc};

use crate::ir::{EdgeParameters, FieldValue};

use futures_util::StreamExt as _;

use super::{
    AsVertex, Typename,
    async_adapter::{AsyncAdapter, ContextOutcomeStream, ContextStream, VertexStream},
};

/// A simplified variant of the [`AsyncAdapter`] trait.
///
/// Implementing `AsyncBasicAdapter` provides a "free" [`AsyncAdapter`] implementation.
/// `AsyncBasicAdapter` gives up a bit of [`AsyncAdapter`]'s flexibility in exchange for being
/// as simple as possible to implement:
/// - `&str` instead of `&Arc<str>` for all names of types, properties, and edges.
/// - Simplified function signatures, with only the minimum necessary arguments.
/// - Automatic handling of the `__typename` special property.
///
/// The easiest way to implement this trait is with the `Vertex` associated type set
/// to an enum that is `#[derive(Debug, Clone, TrustfallEnumVertex)]`.
pub trait AsyncBasicAdapter<'vertex> {
    /// The type of vertices in the dataset this adapter queries.
    /// It's frequently a good idea to use an `Arc<...>` type for cheaper cloning here,
    /// especially since async adapters are often used in `Send`-capable contexts.
    type Vertex: Typename + Clone + Debug + 'vertex;

    /// The error type this adapter may report. See [`AsyncAdapter::Error`].
    type Error: std::error::Error + 'static;

    /// Produce a stream of vertices for the specified starting edge.
    ///
    /// Starting edges are ones where queries are allowed to begin.
    /// They are defined directly on the root query type of the schema.
    /// For example, `User` is the starting edge of the following query:
    /// ```graphql
    /// query {
    ///     User {
    ///         name @output
    ///     }
    /// }
    /// ```
    ///
    /// The caller guarantees that:
    /// - The specified edge is a starting edge in the schema being queried.
    /// - Any parameters the edge requires per the schema have values provided.
    fn resolve_starting_vertices(
        &self,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> VertexStream<'vertex, Result<Self::Vertex, Self::Error>>;

    /// Resolve the value of a vertex property over a stream of query contexts.
    ///
    /// Each [`DataContext`](super::DataContext) in the `contexts` argument has an active vertex,
    /// which is either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This method resolves the property value on that active vertex.
    ///
    /// Unlike the [`AsyncAdapter::resolve_property`] method, this method does not
    /// handle the special `__typename` property. Instead, that property is resolved
    /// by the [`AsyncBasicAdapter::resolve_typename`] method, which has a default
    /// implementation using the [`Typename`] trait implemented by `Self::Vertex`.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - `property_name` is a property field on `type_name` defined in the schema.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned stream must satisfy these properties:
    /// - Produce `(context, outcome)` tuples with the property's value (or an error) for that context.
    /// - Produce contexts in the same order as the input `contexts` stream produced them.
    /// - Produce property values whose type matches the property's type defined in the schema.
    /// - When a context's active vertex is `None`, its property outcome is `Ok(FieldValue::Null)`.
    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeStream<'vertex, V, Result<FieldValue, Self::Error>>;

    /// Resolve the neighboring vertices across an edge, for each query context in a stream.
    ///
    /// Each [`DataContext`](super::DataContext) in the `contexts` argument has an active vertex,
    /// which is either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This method resolves the neighboring vertices for that active vertex.
    ///
    /// If the schema this adapter covers has no edges aside from starting edges,
    /// then this method will never be called and may be implemented as `unreachable!()`.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - `edge_name` is an edge field on `type_name` defined in the schema.
    /// - Any parameters the edge requires per the schema have values provided.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned stream must satisfy these properties:
    /// - Produce `(context, neighbors)` tuples with a stream of neighbor vertices for that edge.
    /// - Produce contexts in the same order as the input `contexts` stream produced them.
    /// - Each neighboring vertex is of the type specified for that edge in the schema.
    /// - When a context's active vertex is `None`, it has an empty neighbors stream.
    #[allow(clippy::type_complexity)]
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &str,
        edge_name: &str,
        parameters: &EdgeParameters,
    ) -> ContextOutcomeStream<'vertex, V, VertexStream<'vertex, Result<Self::Vertex, Self::Error>>>;

    /// Attempt to coerce vertices to a subtype, over a stream of query contexts.
    ///
    /// In this example query, the starting vertices of type `File` are coerced to `AudioFile`:
    /// ```graphql
    /// query {
    ///     File {
    ///         ... on AudioFile {
    ///             duration @output
    ///         }
    ///     }
    /// }
    /// ```
    /// The `... on AudioFile` operator causes only `AudioFile` vertices to be retained,
    /// filtering out all other kinds of `File` vertices.
    ///
    /// Each [`DataContext`](super::DataContext) in the `contexts` argument has an active vertex,
    /// which is either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This method checks whether the active vertex is of the specified subtype.
    ///
    /// If this adapter's schema contains no subtyping, then no type coercions are possible:
    /// this method will never be called and may be implemented as `unreachable!()`.
    ///
    /// The caller guarantees that:
    /// - `type_name` is an interface defined in the schema.
    /// - `coerce_to_type` is a type or interface that implements `type_name` in the schema.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned stream must satisfy these properties:
    /// - Produce `(context, outcome)` tuples showing if the coercion succeeded (or an error).
    /// - Produce contexts in the same order as the input `contexts` stream produced them.
    /// - When a context's active vertex is `None`, its coercion outcome is `Ok(false)`.
    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextStream<'vertex, V>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeStream<'vertex, V, Result<bool, Self::Error>>;

    /// Resolve the `__typename` special property over a stream of query contexts.
    ///
    /// Each [`DataContext`](super::DataContext) in the `contexts` argument has an active vertex,
    /// which is either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This method resolves the name of the type of that active vertex. That type may not always
    /// be the same as the value of the `type_name` parameter, due to inheritance in the schema.
    /// For example, consider a schema with types `interface Message` and
    /// `type Email implements Message`, and a query like the following:
    /// ```graphql
    /// query {
    ///     Message {
    ///         __typename @output
    ///     }
    /// }
    /// ```
    /// The resulting `resolve_typename()` call here would have `type_name = "Message"`.
    /// However, some of the messages read by this query may be emails!
    /// For those messages, outputting `__typename` would produce the value `"Email"`.
    ///
    /// The default implementation uses the [`Typename`] trait implemented by `Self::Vertex`
    /// to get each vertex's type name.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned stream must satisfy these properties:
    /// - Produce `(context, outcome)` tuples with the property's value (or an error) for that context.
    /// - Produce contexts in the same order as the input `contexts` stream produced them.
    /// - Produce property values whose type matches the property's type defined in the schema.
    /// - When a context's active vertex is `None`, its property outcome is `Ok(FieldValue::Null)`.
    ///
    /// # Overriding the default implementation
    ///
    /// Some adapters may be able to implement this method more efficiently than the provided
    /// default implementation.
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
    ) -> ContextOutcomeStream<'vertex, V, VertexStream<'vertex, Result<Self::Vertex, Self::Error>>>
    {
        <Self as AsyncBasicAdapter>::resolve_neighbors(
            self,
            contexts,
            type_name.as_ref(),
            edge_name.as_ref(),
            parameters,
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
