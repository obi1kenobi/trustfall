use std::fmt::Debug;

use crate::ir::{EdgeParameters, Eid, FieldValue, Vid};

use super::{Adapter, DataContext, InterpretedQuery};

/// An iterator of vertices representing data points we are querying.
type VertexIterator<'vertex, VertexT> = Box<dyn Iterator<Item = VertexT> + 'vertex>;

/// An iterator of query contexts: bookkeeping structs we use to build up the query results.
///
/// Each context represents a possible result of the query. At each query processing step,
/// all the contexts at that step have fulfilled all the query conditions thus far.
///
/// This type is usually an input to adapter resolver functions. Calling those functions
/// asks them to resolve a property, edge, or type coercion for the particular vertex
/// the context is currently processing at that point in the query.
type ContextIterator<'vertex, VertexT> = Box<dyn Iterator<Item = DataContext<VertexT>> + 'vertex>;

/// Iterator of (context, outcome) tuples: the output type of most resolver functions.
///
/// Resolver functions produce an output value for each context:
/// - resolve_property() produces that property's value;
/// - resolve_neighbors() produces an iterator of neighboring vertices along an edge;
/// - coerce_vertex_type() gives a bool representing whether the vertex is of the desired type.
///
/// This type lets us write those output types in a slightly more readable way.
type ContextOutcomeIterator<'vertex, VertexT, OutcomeT> =
    Box<dyn Iterator<Item = (DataContext<VertexT>, OutcomeT)> + 'vertex>;

pub trait BasicAdapter<'vertex> {
    type Vertex: Clone + Debug + 'vertex;

    /// Produce an iterator of vertices for the specified starting edge.
    ///
    /// Starting edges are the edges defined on the schema's specified query type,
    /// usually named `RootSchemaQuery`.
    ///
    /// The caller guarantees that:
    /// - The specified edge is defined in the schema being queried.
    /// - Any parameters the edge requires per the schema have values provided.
    fn resolve_starting_vertices(
        &mut self,
        edge_name: &str,
        parameters: Option<&EdgeParameters>,
    ) -> VertexIterator<'vertex, Self::Vertex>;

    /// Resolve the value of a vertex property over an iterator of query contexts.
    ///
    /// Each context in the `data_contexts` argument has an active vertex, which is
    /// either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This function resolves the property value on that active vertex.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - `property_name` is a property field on `type_name` defined in the schema.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, property_value)` tuples with the property's value for that context.
    /// - Produce contexts in the same order as the input `data_contexts` iterator produced them.
    /// - Produce property values whose type matches the property's type defined in the schema.
    /// - When a context's active vertex is `None`, its property value is `FieldValue::Null`.
    fn resolve_property(
        &mut self,
        data_contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &str,
        property_name: &str,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, FieldValue>;

    /// Resolve the neighboring vertices across an edge over an iterator of query contexts.
    ///
    /// Each context in the `data_contexts` argument has an active vertex, which is
    /// either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This function resolves the neighboring vertices for that active vertex.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - `edge_name` is an edge field on `type_name` defined in the schema.
    /// - Any parameters the edge requires per the schema have values provided.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, neighbors)` tuples with an iterator of neighbor vertices for that edge.
    /// - Produce contexts in the same order as the input `data_contexts` iterator produced them.
    /// - Each neighboring vertex is of the type specified for that edge in the schema.
    /// - When a context's active vertex is None, it has an empty neighbors iterator.
    fn resolve_neighbors(
        &mut self,
        data_contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &str,
        edge_name: &str,
        parameters: Option<&EdgeParameters>,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, VertexIterator<'vertex, Self::Vertex>>;

    /// Attempt to coerce vertices to a subtype, over an iterator of query contexts.
    ///
    /// Each context in the `data_contexts` argument has an active vertex, which is
    /// either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This function checks whether the active vertex is of the specified subtype.
    ///
    /// The caller guarantees that:
    /// - `type_name` is an interface defined in the schema.
    /// - `coerce_to_type` is a type or interface that implements `type_name` in the schema.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, can_coerce)` tuples showing if the coercion succeded for that context.
    /// - Produce contexts in the same order as the input `data_contexts` iterator produced them.
    /// - Each neighboring vertex is of the type specified for that edge in the schema.
    /// - When a context's active vertex is `None`, its coercion outcome is `false`.
    fn coerce_vertex_type(
        &mut self,
        data_contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &str,
        coerce_to_type: &str,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, bool>;
}

impl<'token, T> Adapter<'token> for T
where
    T: BasicAdapter<'token>,
{
    type DataToken = T::Vertex;

    fn get_starting_tokens(
        &mut self,
        edge: std::sync::Arc<str>,
        parameters: Option<std::sync::Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'token> {
        <Self as BasicAdapter>::resolve_starting_vertices(
            self,
            edge.as_ref(),
            parameters.as_deref(),
        )
    }

    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: std::sync::Arc<str>,
        field_name: std::sync::Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)> + 'token> {
        <Self as BasicAdapter>::resolve_property(
            self,
            data_contexts,
            current_type_name.as_ref(),
            field_name.as_ref(),
        )
    }

    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: std::sync::Arc<str>,
        edge_name: std::sync::Arc<str>,
        parameters: Option<std::sync::Arc<EdgeParameters>>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
        _edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
                Item = (
                    DataContext<Self::DataToken>,
                    Box<dyn Iterator<Item = Self::DataToken> + 'token>,
                ),
            > + 'token,
    > {
        <Self as BasicAdapter>::resolve_neighbors(
            self,
            data_contexts,
            current_type_name.as_ref(),
            edge_name.as_ref(),
            parameters.as_deref(),
        )
    }

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: std::sync::Arc<str>,
        coerce_to_type_name: std::sync::Arc<str>,
        _query_hint: InterpretedQuery,
        _vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'token> {
        <Self as BasicAdapter>::coerce_vertex_type(
            self,
            data_contexts,
            current_type_name.as_ref(),
            coerce_to_type_name.as_ref(),
        )
    }
}
