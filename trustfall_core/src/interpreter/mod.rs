use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use itertools::Itertools;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    ir::{EdgeParameters, Eid, FieldRef, FieldValue, IndexedQuery, Type, Vid},
    util::BTreeMapTryInsertExt,
};

use self::error::QueryArgumentsError;

pub mod basic_adapter;
pub mod error;
pub mod execution;
mod filtering;
pub mod helpers;
mod hints;
pub mod replay;
pub mod trace;

pub use hints::{
    CandidateValue, DynamicallyResolvedValue, EdgeInfo, NeighborInfo, QueryInfo, Range,
    RequiredProperty, ResolveEdgeInfo, ResolveInfo, VertexInfo,
};

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

/// Accessor method for the `__typename` special property of Trustfall vertices.
pub trait Typename {
    /// Returns the type name of this vertex in the Trustfall query graph.
    ///
    /// Corresponds to the `__typename` special property of Trustfall vertices.
    fn typename(&self) -> &'static str;
}

/// A tagged value captured and imported from another query component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum TaggedValue {
    /// This tagged value comes from an @optional scope that didn't exist.
    /// All comparisons against it should succeed, per our spec.
    NonexistentOptional,

    /// This tagged value was resolved to the specified value.
    Some(FieldValue),
}

/// A partial result of a Trustfall query within the interpreter defined in this module.
#[derive(Debug, Clone)]
pub struct DataContext<Vertex> {
    active_vertex: Option<Vertex>,
    vertices: BTreeMap<Vid, Option<Vertex>>,
    values: Vec<FieldValue>,
    suspended_vertices: Vec<Option<Vertex>>,
    folded_contexts: BTreeMap<Eid, Option<Vec<DataContext<Vertex>>>>,
    folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>>,
    piggyback: Option<Vec<DataContext<Vertex>>>,
    imported_tags: BTreeMap<FieldRef, TaggedValue>,
}

impl<Vertex> DataContext<Vertex> {
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
    pub fn active_vertex<V>(&self) -> Option<&V>
    where
        Vertex: AsVertex<V>,
    {
        self.active_vertex.as_ref().and_then(AsVertex::as_vertex)
    }

    /// Converts `DataContext<Vertex>` to `DataContext<Other>` by mapping each `Vertex` to `Other`.
    ///
    /// If you are implementing an [`Adapter`] for a data source,
    /// you almost certainly *should not* be using this function.
    /// You're probably looking for [`DataContext::active_vertex()`] instead.
    pub fn map<Other>(self, mapper: &mut impl FnMut(Vertex) -> Other) -> DataContext<Other> {
        DataContext {
            active_vertex: self.active_vertex.map(&mut *mapper),
            vertices: self.vertices.into_iter().map(|(k, v)| (k, v.map(&mut *mapper))).collect(),
            values: self.values,
            suspended_vertices: self
                .suspended_vertices
                .into_iter()
                .map(|v| v.map(&mut *mapper))
                .collect(),
            folded_contexts: self
                .folded_contexts
                .into_iter()
                .map(|(k, ctxs)| {
                    (k, ctxs.map(|v| v.into_iter().map(|ctx| ctx.map(&mut *mapper)).collect()))
                })
                .collect(),
            folded_values: self.folded_values,
            piggyback: self
                .piggyback
                .map(|v| v.into_iter().map(|ctx| ctx.map(&mut *mapper)).collect()),
            imported_tags: self.imported_tags,
        }
    }

    /// Map each `Vertex` to `Option<Other>`, thus converting `Self` to `DataContext<Other>`.
    ///
    /// This is the [`DataContext`] equivalent of [`Option::and_then`][option], which is also
    /// referred to as "flat-map" in some languages.
    ///
    /// If you are implementing an [`Adapter`] for a data source,
    /// you almost certainly *should not* be using this function.
    /// You're probably looking for [`DataContext::active_vertex()`] instead.
    ///
    /// [option]: https://doc.rust-lang.org/std/option/enum.Option.html#method.and_then
    pub fn flat_map<T>(self, mapper: &mut impl FnMut(Vertex) -> Option<T>) -> DataContext<T> {
        DataContext {
            active_vertex: self.active_vertex.and_then(&mut *mapper),
            vertices: self
                .vertices
                .into_iter()
                .map(|(k, v)| (k, v.and_then(&mut *mapper)))
                .collect::<BTreeMap<Vid, Option<T>>>(),
            values: self.values,
            suspended_vertices: self
                .suspended_vertices
                .into_iter()
                .map(|v| v.and_then(&mut *mapper))
                .collect(),
            folded_contexts: self
                .folded_contexts
                .into_iter()
                .map(|(k, ctxs)| {
                    (k, ctxs.map(|v| v.into_iter().map(|ctx| ctx.flat_map(&mut *mapper)).collect()))
                })
                .collect(),
            folded_values: self.folded_values,
            piggyback: self
                .piggyback
                .map(|v| v.into_iter().map(|ctx| ctx.flat_map(&mut *mapper)).collect()),
            imported_tags: self.imported_tags,
        }
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
#[serde(bound = "Vertex: Debug + Clone + Serialize + DeserializeOwned")]
struct SerializableContext<Vertex> {
    active_vertex: Option<Vertex>,
    vertices: BTreeMap<Vid, Option<Vertex>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    values: Vec<FieldValue>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    suspended_vertices: Vec<Option<Vertex>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folded_contexts: BTreeMap<Eid, Option<Vec<DataContext<Vertex>>>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    piggyback: Option<Vec<DataContext<Vertex>>>,

    /// Tagged values imported from an ancestor component of the one currently being evaluated.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    imported_tags: BTreeMap<FieldRef, TaggedValue>,
}

impl<Vertex> From<SerializableContext<Vertex>> for DataContext<Vertex> {
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

impl<Vertex> From<DataContext<Vertex>> for SerializableContext<Vertex> {
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
        self.vertices.insert_or_error(vid, self.active_vertex.clone()).unwrap();
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

impl<Vertex: PartialEq> PartialEq for DataContext<Vertex> {
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

impl<Vertex: Eq> Eq for DataContext<Vertex> {}

impl<Vertex> Serialize for DataContext<Vertex>
where
    Vertex: Debug + Clone + Serialize + DeserializeOwned,
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
    Vertex: Debug + Clone + Serialize + DeserializeOwned,
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
                missing_arguments.into_iter().map(|x| x.to_string()).collect(),
            ));
        }

        let unused_arguments = arguments
            .keys()
            .map(|x| x.as_ref())
            .filter(|arg| !indexed_query.ir_query.variables.contains_key(*arg))
            .collect_vec();
        if !unused_arguments.is_empty() {
            errors.push(QueryArgumentsError::UnusedArguments(
                unused_arguments.into_iter().map(|x| x.to_string()).collect(),
            ));
        }

        if errors.is_empty() {
            Ok(Self { indexed_query, arguments })
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
    if variable_type.is_valid_value(argument_value) {
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
/// The most straightforward way to implement this trait is to use
/// the [`trustfall_stubgen` code-generator tool][stubgen] tool to auto-generate stubs
/// customized to match your dataset's schema, then fill in the blanks denoted by `todo!()`.
///
/// If you prefer to implement the trait without code generation, consider implementing
/// [`BasicAdapter`](self::basic_adapter::BasicAdapter) instead. That's a simpler version
/// of this trait and can be faster to implement without a significant loss of functionality:
/// - Both traits support the same set of queries. Under the hood,
///   [`BasicAdapter`](self::basic_adapter::BasicAdapter) itself implements [`Adapter`].
/// - If you need optimizations like batching or caching, you can implement them within
///   [`BasicAdapter`](self::basic_adapter::BasicAdapter) as well.
/// - If you need more advanced optimizations such as predicate pushdown, or need to access
///   Trustfall's static analysis capabilities, implement this trait directly instead.
///
/// [stubgen]: https://docs.rs/trustfall_stubgen/latest/trustfall_stubgen/
pub trait Adapter<'vertex> {
    /// The type of vertices in the dataset this adapter queries.
    /// Unless your intended vertex type is cheap to clone, consider wrapping it an [`Rc`][rc]
    /// or [`Arc`] to make cloning it cheaper since that's a fairly common operation
    /// when queries are evaluated.
    ///
    /// [rc]: std::rc::Rc
    type Vertex: Clone + Debug + 'vertex;

    /// Produce an iterator of vertices for the specified starting edge.
    ///
    /// Starting edges are the entry points for querying according to a schema.
    /// Each query starts at such an edge, and such starting edges are defined
    /// directly on the root query type of the schema.
    ///
    /// # Example
    ///
    /// Consider this query which gets the URLs of the posts
    /// currently on the front page of HackerNews: [playground][playground]
    /// ```graphql
    /// query {
    ///   FrontPage {
    ///     url @output
    ///   }
    /// }
    /// ```
    ///
    /// The [HackerNews schema][schema] defines `FrontPage` as a starting edge
    /// that points to vertices of type `Item`.
    ///
    /// As part of executing this query, Trustfall will call this method
    /// with `edge_name = "FrontPage"`. Here's the [implementation of this method][method]
    /// in the HackerNews example adapter.
    ///
    /// # Preconditions and postconditions
    ///
    /// The caller guarantees that:
    /// - The specified edge is a starting edge in the schema being queried.
    /// - Any parameters the edge requires per the schema have values provided.
    ///
    /// [playground]: https://play.predr.ag/hackernews#?f=2&q=*3-Get-the-HackerNews-item-URLs-of-the-items*l*3-currently-on-the-front-page.*lquery---0FrontPage---2url-*o*l--_0*J*l*J&v=--0*l*J
    /// [schema]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/hackernews.graphql#L35
    /// [method]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/adapter.rs#L127-L133
    fn resolve_starting_vertices(
        &self,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveInfo,
    ) -> VertexIterator<'vertex, Self::Vertex>;

    /// Resolve a property required by the query that's being evaluated.
    ///
    /// Each [`DataContext`] in the `contexts` parameter has an active vertex
    /// [`DataContext::active_vertex()`]. This call is asking for the value of
    /// the specified property on each such active vertex,
    /// for each active vertex in the input iterator.
    ///
    /// The most ergonomic way to implement this method is usually via
    /// the [`resolve_property_with()`][resolve-property] helper method together with
    /// the [`field_property!()`][field-property] and [`accessor_property!()`][accessor-property]
    /// macros.
    ///
    /// # Example
    ///
    /// Consider this query which gets the URLs of the posts
    /// currently on the front page of HackerNews: [playground][playground]
    /// ```graphql
    /// query {
    ///   FrontPage {
    ///     url @output
    ///   }
    /// }
    /// ```
    ///
    /// Our HackerNews schema [defines][starting-edge] `FrontPage` as a starting edge
    /// that points to vertices of type `Item`, and [defines][property] `url`
    /// as a property on the `Item` type.
    ///
    /// As part of executing this query, Trustfall will call this method
    /// with `type_name = "Item"` and `property_name = "url"`.
    /// This is how Trustfall looks up the URLs of the items returned by this query.
    /// Here's the [implementation of this method][method] in the HackerNews example adapter.
    ///
    /// # Preconditions and postconditions
    ///
    /// The active vertex may be `None`, or a `Some(v)` whose `v` is of Rust type `&Self::Vertex`
    /// and represents a vertex whose type in the Trustfall schema is given by
    /// this function's `type_name` parameter.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - `property_name` is either a property field on `type_name` defined in the schema,
    ///   or the special value `"__typename"` requesting the name of the vertex's type.
    /// - When the active vertex is `Some(...)`, its represents a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface implemented by
    ///   the vertex's type.
    ///
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, property_value)` tuples with the property's value for that context.
    /// - Produce contexts in the same order as the input `contexts` iterator produced them.
    /// - Produce property values whose type matches the property's type defined in the schema.
    /// - When a context's active vertex is `None`, its property value is [`FieldValue::Null`].
    ///
    /// [playground]: https://play.predr.ag/hackernews#?f=2&q=*3-Get-the-HackerNews-item-URLs-of-the-items*l*3-currently-on-the-front-page.*lquery---0FrontPage---2url-*o*l--_0*J*l*J&v=--0*l*J
    /// [starting-edge]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/hackernews.graphql#L35
    /// [property]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/hackernews.graphql#L44
    /// [method]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/adapter.rs#L151
    /// [resolve-property]: helpers::resolve_property_with
    /// [field-property]: crate::field_property
    /// [accessor-property]: crate::accessor_property
    fn resolve_property<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextIterator<'vertex, V>,
        type_name: &Arc<str>,
        property_name: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'vertex, V, FieldValue>;

    /// Resolve the neighboring vertices across an edge.
    ///
    /// Each [`DataContext`] in the `contexts` parameter has an active vertex
    /// [`DataContext::active_vertex()`]. This call is asking for
    /// the iterator of neighboring vertices of the active vertex along a specified edge,
    /// for each active vertex in the input iterator.
    ///
    /// The most ergonomic way to implement this method is usually via
    /// the [`resolve_neighbors_with()`][resolve-neighbors] helper method.
    ///
    /// # Example
    ///
    /// Consider this query which gets the usernames and karma points of the users
    /// who submitted the latest stories on HackerNews: [playground][playground]
    /// ```graphql
    /// query {
    ///   Latest {
    ///     byUser {
    ///       id @output
    ///       karma @output
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Our HackerNews schema [defines][starting-edge] `Latest` as a starting edge
    /// that points to vertices of type `Story`.
    /// In turn, `Story` [has an edge][edge] called `byUser` that points to `User` vertices.
    ///
    /// As part of executing this query, Trustfall will call this method
    /// with `type_name = "Story"` and `edge_name = "byUser"`.
    /// This is how Trustfall looks up the user vertices representing the submitters
    /// of the latest HackerNews stories.
    /// Here's the [implementation of this method][method] in the HackerNews example adapter.
    ///
    /// # Preconditions and postconditions
    ///
    /// The active vertex may be `None`, or a `Some(v)` whose `v` is of Rust type `&Self::Vertex`
    /// and represents a vertex whose type in the Trustfall schema is given by
    /// this function's `type_name` parameter.
    ///
    /// If the schema this adapter covers has no edges aside from starting edges,
    /// then this method will never be called and may be implemented as `unreachable!()`.
    ///
    /// The caller guarantees that:
    /// - `type_name` is a type or interface defined in the schema.
    /// - `edge_name` is an edge field on `type_name` defined in the schema.
    /// - Each parameter required by the edge has a value of appropriate type, per the schema.
    /// - When the active vertex is `Some(...)`, its represents a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface implemented by
    ///   the vertex's type.
    ///
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, neighbors)` tuples with an iterator of neighbor vertices for that edge.
    /// - Produce contexts in the same order as the input `contexts` iterator produced them.
    /// - Each neighboring vertex is of the type specified for that edge in the schema.
    /// - When a context's active vertex is None, it has an empty neighbors iterator.
    ///
    /// [playground]: https://play.predr.ag/hackernews#?f=2&q=*3-Get-the-usernames-and-karma-points-of-the-folks*l*3-who-submitted-the-latest-stories-on-HackerNews.*lquery---0Latest---2byUser---4id-*o*l--_4karma-*o*l--_2--*0*J*l*J&v=--0*l*J
    /// [starting-edge]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/hackernews.graphql#L37
    /// [edge]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/hackernews.graphql#L73
    /// [method]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/adapter.rs#L223
    /// [resolve-neighbors]: helpers::resolve_neighbors_with
    fn resolve_neighbors<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextIterator<'vertex, V>,
        type_name: &Arc<str>,
        edge_name: &Arc<str>,
        parameters: &EdgeParameters,
        resolve_info: &ResolveEdgeInfo,
    ) -> ContextOutcomeIterator<'vertex, V, VertexIterator<'vertex, Self::Vertex>>;

    /// Attempt to coerce vertices to a subtype, as required by the query that's being evaluated.
    ///
    /// Each [`DataContext`] in the `contexts` parameter has an active vertex
    /// [`DataContext::active_vertex()`]. This call is asking whether the active vertex
    /// happens to be an instance of a subtype, for each active vertex in the input iterator.
    ///
    /// The most ergonomic ways to implement this method usually rely on
    /// the [`resolve_coercion_using_schema()`][resolve-schema]
    /// or [`resolve_coercion_with()`][resolve-basic] helper methods.
    ///
    /// # Example
    ///
    /// Consider this query which gets the titles of all stories on the front page of HackerNews,
    /// while discarding non-story items such as job postings and polls: [playground][playground]
    /// ```graphql
    /// query {
    ///   FrontPage {
    ///     ... on Story {
    ///       title @output
    ///     }
    ///   }
    /// }
    /// ```
    ///
    /// Our HackerNews schema [defines][starting-edge] `FrontPage` as a starting edge
    /// that points to vertices of type `Item`.
    /// It also defines `Story` as [a subtype][subtype] of `Item`.
    ///
    /// After resolving the `FrontPage` starting edge, Trustfall will need to determine which
    /// of the resulting `Item` vertices are actually of type `Story`.
    /// This is when Trustfall will call this method
    /// with `type_name = "Item"` and `coerce_to_type = "Story"`.
    /// Here's the [implementation of this method][method] in the HackerNews example adapter.
    ///
    /// # Preconditions and postconditions
    ///
    /// The active vertex may be `None`, or a `Some(v)` whose `v` is of Rust type `&Self::Vertex`
    /// and represents a vertex whose type in the Trustfall schema is given by
    /// this function's `type_name` parameter.
    ///
    /// If this adapter's schema contains no subtyping, then no type coercions are possible:
    /// this method will never be called and may be implemented as `unreachable!()`.
    ///
    /// The caller guarantees that:
    /// - `type_name` is an interface defined in the schema.
    /// - `coerce_to_type` is a type or interface that implements `type_name` in the schema.
    /// - When the active vertex is `Some(...)`, its represents a vertex of type `type_name`:
    ///   either its type is exactly `type_name`, or `type_name` is an interface implemented by
    ///   the vertex's type.
    ///
    /// The returned iterator must satisfy these properties:
    /// - Produce `(context, can_coerce)` tuples showing if the coercion succeded for that context.
    /// - Produce contexts in the same order as the input `contexts` iterator produced them.
    /// - Each neighboring vertex is of the type specified for that edge in the schema.
    /// - When a context's active vertex is `None`, its coercion outcome is `false`.
    ///
    /// [playground]: https://play.predr.ag/hackernews#?f=2&q=*3-Get-the-title-of-stories-on-the-HN-front-page.*l*3-Discards-any-non*-story-items-on-the-front-page*L*l*3-such-as-job-postings-or-polls.*lquery---0FrontPage---2*E-Story---4title-*o*l--_2--*0*J*l*J&v=--0*l*J
    /// [starting-edge]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/hackernews.graphql#L35
    /// [subtype]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/hackernews.graphql#L58
    /// [method]: https://github.com/obi1kenobi/trustfall/blob/main/trustfall/examples/hackernews/adapter.rs#L375
    /// [resolve-schema]: helpers::resolve_coercion_using_schema
    /// [resolve-basic]: helpers::resolve_coercion_with
    fn resolve_coercion<V: AsVertex<Self::Vertex> + 'vertex>(
        &self,
        contexts: ContextIterator<'vertex, V>,
        type_name: &Arc<str>,
        coerce_to_type: &Arc<str>,
        resolve_info: &ResolveInfo,
    ) -> ContextOutcomeIterator<'vertex, V, bool>;
}

/// Attempt to dereference a value to a `&V`, returning `None` if the value did not contain a `V`.
///
/// This trait allows types that may contain a `V` to be projected down to a `Option<&V>`.
/// It's similar in spirit to the built-in [`Deref`][deref] trait.
/// The primary difference is that [`AsVertex`] does not guarantee it'll be able to produce a `&V`,
/// instead returning `Option<&V>`. The same type may implement [`AsVertex<V>`] multiple times
/// with different `V` types, also unlike [`Deref`][deref].
///
/// [deref]: https://doc.rust-lang.org/std/ops/trait.Deref.html
pub trait AsVertex<V>: Debug + Clone {
    /// Dereference this value into a `&V`, if the value happens to contain a `V`.
    ///
    /// If this method returns `Some(&v)`, [`AsVertex::into_vertex()`] for the same `V`
    /// is guaranteed to return `Some(v)` as well.
    fn as_vertex(&self) -> Option<&V>;

    /// Consume `self` and produce the contained `V`, if one was indeed present.
    ///
    /// If this method returned `Some(v)`, prior [`AsVertex::as_vertex()`] calls for the same `V`
    /// are guaranteed to have returned `Some(&v)` as well.
    fn into_vertex(self) -> Option<V>;
}

/// Allow bidirectional conversions between a type `V` and the type implementing this trait.
///
/// Values of type `V` may be converted into the type implementing this trait, and values
/// of the implementing type may be converted into `V` via the `AsVertex<V>` supertrait.
pub trait Cast<V>: AsVertex<V> {
    /// Convert a `V` into the type implementing this trait.
    ///
    /// This is the inverse of [`AsVertex::into_vertex()`]: this function converts `vertex: V`
    /// into `Self` whereas [`AsVertex::into_vertex()`] can convert the resulting `Self`
    /// back into `Some(v)`.
    fn into_self(vertex: V) -> Self;
}

/// Trivially, every `Debug + Clone` type is [`AsVertex`] of itself.
impl<V: Debug + Clone> AsVertex<V> for V {
    fn as_vertex(&self) -> Option<&V> {
        Some(self)
    }

    fn into_vertex(self) -> Option<V> {
        Some(self)
    }
}
