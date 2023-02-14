use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use async_graphql_parser::types::Type;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    ir::{
        indexed::IndexedQuery, types::is_argument_type_valid, EdgeParameters, Eid, FieldRef,
        FieldValue, Vid,
    },
    util::BTreeMapTryInsertExt,
};

use self::error::QueryArgumentsError;

pub mod basic_adapter;
pub mod error;
pub mod execution;
mod filtering;
pub mod helpers;
mod hints;
pub mod macros;
pub mod replay;
pub mod trace;

pub use hints::QueryInfo;

/// An iterator of vertices representing data points we are querying.
pub type VertexIterator<'vertex, VertexT> = Box<dyn Iterator<Item = VertexT> + 'vertex>;

/// An iterator of query contexts: bookkeeping structs we use to build up the query results.
///
/// Each context represents a possible result of the query. At each query processing step,
/// all the contexts at that step have fulfilled all the query conditions thus far.
///
/// This type is usually an input to adapter resolver functions. Calling those functions
/// asks them to resolve a property, edge, or type coercion for the particular vertex
/// the context is currently processing at that point in the query.
pub type ContextIterator<'vertex, VertexT> = VertexIterator<'vertex, DataContext<VertexT>>;

/// Iterator of (context, outcome) tuples: the output type of most resolver functions.
///
/// Resolver functions produce an output value for each context:
/// - resolve_property() produces that property's value;
/// - resolve_neighbors() produces an iterator of neighboring vertices along an edge;
/// - resolve_coercion() gives a bool representing whether the vertex is of the desired type.
///
/// This type lets us write those output types in a slightly more readable way.
pub type ContextOutcomeIterator<'vertex, VertexT, OutcomeT> =
    Box<dyn Iterator<Item = (DataContext<VertexT>, OutcomeT)> + 'vertex>;

/// A partial result of a Trustfall query within the interpreter defined in this module.
#[derive(Debug, Clone)]
pub struct DataContext<Vertex: Clone + Debug> {
    active_vertex: Option<Vertex>,
    vertices: BTreeMap<Vid, Option<Vertex>>,
    values: Vec<FieldValue>,
    suspended_vertices: Vec<Option<Vertex>>,
    folded_contexts: BTreeMap<Eid, Vec<DataContext<Vertex>>>,
    folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>>,
    piggyback: Option<Vec<DataContext<Vertex>>>,
    imported_tags: BTreeMap<FieldRef, FieldValue>,
}

impl<Vertex: Clone + Debug> DataContext<Vertex> {
    /// The vertex currently being processed.
    ///
    /// For contexts passed to an [`Adapter`] resolver method,
    /// this is the vertex whose data needs to be resolved.
    ///
    /// The active vertex may be `None` when processing an `@optional` part
    /// of a Trustfall query whose data did not exist. In that case:
    /// - [`Adapter::resolve_property`] must produce [`FieldValue::Null`] for that context.
    /// - [`Adapter::resolve_neighbors`] must produce an empty iterator of neighbors
    ///   such as `Box::new(std::iter::empty())` for that context.
    /// - [`Adapter::resolve_coercion`] must produce a `false` coercion outcome for that context.
    pub fn active_vertex(&self) -> Option<&Vertex> {
        self.active_vertex.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum ValueOrVec {
    Value(FieldValue),
    Vec(Vec<ValueOrVec>),
}

impl ValueOrVec {
    fn as_mut_vec(&mut self) -> Option<&mut Vec<ValueOrVec>> {
        match self {
            ValueOrVec::Value(_) => None,
            ValueOrVec::Vec(v) => Some(v),
        }
    }
}

impl From<ValueOrVec> for FieldValue {
    fn from(v: ValueOrVec) -> Self {
        match v {
            ValueOrVec::Value(value) => value,
            ValueOrVec::Vec(v) => v.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound = "Vertex: Serialize, for<'de2> Vertex: Deserialize<'de2>")]
struct SerializableContext<Vertex>
where
    Vertex: Clone + Debug + Serialize,
    for<'d> Vertex: Deserialize<'d>,
{
    active_vertex: Option<Vertex>,
    vertices: BTreeMap<Vid, Option<Vertex>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    values: Vec<FieldValue>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    suspended_vertices: Vec<Option<Vertex>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folded_contexts: BTreeMap<Eid, Vec<DataContext<Vertex>>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    piggyback: Option<Vec<DataContext<Vertex>>>,

    /// Tagged values imported from an ancestor component of the one currently being evaluated.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    imported_tags: BTreeMap<FieldRef, FieldValue>,
}

impl<Vertex> From<SerializableContext<Vertex>> for DataContext<Vertex>
where
    Vertex: Clone + Debug + Serialize,
    for<'d> Vertex: Deserialize<'d>,
{
    fn from(context: SerializableContext<Vertex>) -> Self {
        Self {
            active_vertex: context.active_vertex,
            vertices: context.vertices,
            values: context.values,
            suspended_vertices: context.suspended_vertices,
            folded_contexts: context.folded_contexts,
            folded_values: context.folded_values,
            piggyback: context.piggyback,
            imported_tags: context.imported_tags,
        }
    }
}

impl<Vertex> From<DataContext<Vertex>> for SerializableContext<Vertex>
where
    Vertex: Clone + Debug + Serialize,
    for<'d> Vertex: Deserialize<'d>,
{
    fn from(context: DataContext<Vertex>) -> Self {
        Self {
            active_vertex: context.active_vertex,
            vertices: context.vertices,
            values: context.values,
            suspended_vertices: context.suspended_vertices,
            folded_contexts: context.folded_contexts,
            folded_values: context.folded_values,
            piggyback: context.piggyback,
            imported_tags: context.imported_tags,
        }
    }
}

impl<Vertex: Clone + Debug> DataContext<Vertex> {
    pub fn new(vertex: Option<Vertex>) -> DataContext<Vertex> {
        DataContext {
            active_vertex: vertex,
            piggyback: None,
            vertices: Default::default(),
            values: Default::default(),
            suspended_vertices: Default::default(),
            folded_contexts: Default::default(),
            folded_values: Default::default(),
            imported_tags: Default::default(),
        }
    }

    fn record_vertex(&mut self, vid: Vid) {
        self.vertices
            .insert_or_error(vid, self.active_vertex.clone())
            .unwrap();
    }

    fn activate_vertex(self, vid: &Vid) -> DataContext<Vertex> {
        DataContext {
            active_vertex: self.vertices[vid].clone(),
            vertices: self.vertices,
            values: self.values,
            suspended_vertices: self.suspended_vertices,
            folded_contexts: self.folded_contexts,
            folded_values: self.folded_values,
            piggyback: self.piggyback,
            imported_tags: self.imported_tags,
        }
    }

    fn split_and_move_to_vertex(&self, new_vertex: Option<Vertex>) -> DataContext<Vertex> {
        DataContext {
            active_vertex: new_vertex,
            vertices: self.vertices.clone(),
            values: self.values.clone(),
            suspended_vertices: self.suspended_vertices.clone(),
            folded_contexts: self.folded_contexts.clone(),
            folded_values: self.folded_values.clone(),
            piggyback: None,
            imported_tags: self.imported_tags.clone(),
        }
    }

    fn move_to_vertex(self, new_vertex: Option<Vertex>) -> DataContext<Vertex> {
        DataContext {
            active_vertex: new_vertex,
            vertices: self.vertices,
            values: self.values,
            suspended_vertices: self.suspended_vertices,
            folded_contexts: self.folded_contexts,
            folded_values: self.folded_values,
            piggyback: self.piggyback,
            imported_tags: self.imported_tags,
        }
    }

    fn ensure_suspended(mut self) -> DataContext<Vertex> {
        if let Some(vertex) = self.active_vertex {
            self.suspended_vertices.push(Some(vertex));
            DataContext {
                active_vertex: None,
                vertices: self.vertices,
                values: self.values,
                suspended_vertices: self.suspended_vertices,
                folded_contexts: self.folded_contexts,
                folded_values: self.folded_values,
                piggyback: self.piggyback,
                imported_tags: self.imported_tags,
            }
        } else {
            self
        }
    }

    fn ensure_unsuspended(mut self) -> DataContext<Vertex> {
        match self.active_vertex {
            None => {
                let active_vertex = self.suspended_vertices.pop().unwrap();
                DataContext {
                    active_vertex,
                    vertices: self.vertices,
                    values: self.values,
                    suspended_vertices: self.suspended_vertices,
                    folded_contexts: self.folded_contexts,
                    folded_values: self.folded_values,
                    piggyback: self.piggyback,
                    imported_tags: self.imported_tags,
                }
            }
            Some(_) => self,
        }
    }
}

impl<Vertex: Debug + Clone + PartialEq> PartialEq for DataContext<Vertex> {
    fn eq(&self, other: &Self) -> bool {
        self.active_vertex == other.active_vertex
            && self.vertices == other.vertices
            && self.values == other.values
            && self.suspended_vertices == other.suspended_vertices
            && self.folded_contexts == other.folded_contexts
            && self.piggyback == other.piggyback
            && self.imported_tags == other.imported_tags
    }
}

impl<Vertex: Debug + Clone + PartialEq + Eq> Eq for DataContext<Vertex> {}

impl<Vertex> Serialize for DataContext<Vertex>
where
    Vertex: Debug + Clone + Serialize,
    for<'d> Vertex: Deserialize<'d>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // TODO: eventually maybe write a proper (de)serialize?
        SerializableContext::from(self.clone()).serialize(serializer)
    }
}

impl<'de, Vertex> Deserialize<'de> for DataContext<Vertex>
where
    Vertex: Debug + Clone + Serialize,
    for<'d> Vertex: Deserialize<'d>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // TODO: eventually maybe write a proper (de)serialize?
        SerializableContext::deserialize(deserializer).map(DataContext::from)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterpretedQuery {
    pub indexed_query: Arc<IndexedQuery>,
    pub arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
}

impl InterpretedQuery {
    #[inline]
    pub fn from_query_and_arguments(
        indexed_query: Arc<IndexedQuery>,
        arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
    ) -> Result<Self, QueryArgumentsError> {
        let mut errors = vec![];

        let mut missing_arguments = vec![];
        for (variable_name, variable_type) in &indexed_query.ir_query.variables {
            match arguments.get(variable_name) {
                Some(argument_value) => {
                    // Ensure the provided argument value is valid for the variable's inferred type.
                    if let Err(e) = validate_argument_type(
                        variable_name.as_ref(),
                        variable_type,
                        argument_value,
                    ) {
                        errors.push(e);
                    }
                }
                None => {
                    missing_arguments.push(variable_name.as_ref());
                }
            }
        }
        if !missing_arguments.is_empty() {
            errors.push(QueryArgumentsError::MissingArguments(
                missing_arguments
                    .into_iter()
                    .map(|x| x.to_string())
                    .collect(),
            ));
        }

        let unused_arguments = arguments
            .keys()
            .map(|x| x.as_ref())
            .filter(|arg| !indexed_query.ir_query.variables.contains_key(*arg))
            .collect_vec();
        if !unused_arguments.is_empty() {
            errors.push(QueryArgumentsError::UnusedArguments(
                unused_arguments
                    .into_iter()
                    .map(|x| x.to_string())
                    .collect(),
            ));
        }

        if errors.is_empty() {
            Ok(Self {
                indexed_query,
                arguments,
            })
        } else {
            Err(errors.into())
        }
    }
}

fn validate_argument_type(
    variable_name: &str,
    variable_type: &Type,
    argument_value: &FieldValue,
) -> Result<(), QueryArgumentsError> {
    if is_argument_type_valid(variable_type, argument_value) {
        Ok(())
    } else {
        Err(QueryArgumentsError::ArgumentTypeError(
            variable_name.to_string(),
            variable_type.to_string(),
            argument_value.to_owned(),
        ))
    }
}

/// Trustfall data providers implement this trait to enable querying their data sets.
///
/// Simpler variants of this trait exist, at the expense of some flexibility.
/// See [`BasicAdapter`](self::basic_adapter::BasicAdapter) for details.
pub trait Adapter<'vertex> {
    /// The type of vertices in the dataset this adapter queries.
    /// It's frequently a good idea to use an Rc<...> type for cheaper cloning here.
    type Vertex: Clone + Debug + 'vertex;

    /// Produce an iterator of vertices for the specified starting edge.
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
        &mut self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        query_info: &QueryInfo,
    ) -> VertexIterator<'vertex, Self::Vertex>;

    /// Resolve the value of a vertex property over an iterator of query contexts.
    ///
    /// Each [`DataContext`](self::DataContext) in the `contexts` argument has an active vertex,
    /// which is either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This function resolves the property value on that active vertex.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - `property_name` is either a property field on `type_name` defined in the schema,
    ///   or the special value `"__typename"` requesting the name of the vertex's type.
    /// - When the active vertex is `Some(...)`, it's a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface that
    ///   the vertex's type implements.
    ///
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, property_value)` tuples with the property's value for that context.
    /// - Produce contexts in the same order as the input `contexts` iterator produced them.
    /// - Produce property values whose type matches the property's type defined in the schema.
    /// - When a context's active vertex is `None`, its property value is [`FieldValue::Null`].
    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, FieldValue>;

    /// Resolve the neighboring vertices across an edge, for each query context in an iterator.
    ///
    /// Each [`DataContext`](self::DataContext) in the `contexts` argument has an active vertex,
    /// which is either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This function resolves the neighboring vertices for that active vertex.
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
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, neighbors)` tuples with an iterator of neighbor vertices for that edge.
    /// - Produce contexts in the same order as the input `contexts` iterator produced them.
    /// - Each neighboring vertex is of the type specified for that edge in the schema.
    /// - When a context's active vertex is None, it has an empty neighbors iterator.
    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, VertexIterator<'vertex, Self::Vertex>>;

    /// Attempt to coerce vertices to a subtype, over an iterator of query contexts.
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
    /// Each [`DataContext`](self::DataContext) in the `contexts` argument has an active vertex,
    /// which is either `None`, or a `Some(Self::Vertex)` value representing a vertex
    /// of type `type_name` defined in the schema.
    ///
    /// This function checks whether the active vertex is of the specified subtype.
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
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, can_coerce)` tuples showing if the coercion succeded for that context.
    /// - Produce contexts in the same order as the input `contexts` iterator produced them.
    /// - Each neighboring vertex is of the type specified for that edge in the schema.
    /// - When a context's active vertex is `None`, its coercion outcome is `false`.
    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        query_info: &QueryInfo,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, bool>;
}
