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
pub mod macros;
pub mod replay;
pub mod trace;

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

#[derive(Debug, Clone)]
pub struct DataContext<Vertex: Clone + Debug> {
    pub current_token: Option<Vertex>,
    tokens: BTreeMap<Vid, Option<Vertex>>,
    values: Vec<FieldValue>,
    suspended_tokens: Vec<Option<Vertex>>,
    folded_contexts: BTreeMap<Eid, Vec<DataContext<Vertex>>>,
    folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>>,
    piggyback: Option<Vec<DataContext<Vertex>>>,
    imported_tags: BTreeMap<FieldRef, FieldValue>,
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
    current_token: Option<Vertex>,
    tokens: BTreeMap<Vid, Option<Vertex>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    values: Vec<FieldValue>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    suspended_tokens: Vec<Option<Vertex>>,

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
            current_token: context.current_token,
            tokens: context.tokens,
            values: context.values,
            suspended_tokens: context.suspended_tokens,
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
            current_token: context.current_token,
            tokens: context.tokens,
            values: context.values,
            suspended_tokens: context.suspended_tokens,
            folded_contexts: context.folded_contexts,
            folded_values: context.folded_values,
            piggyback: context.piggyback,
            imported_tags: context.imported_tags,
        }
    }
}

impl<Vertex: Clone + Debug> DataContext<Vertex> {
    pub fn new(token: Option<Vertex>) -> DataContext<Vertex> {
        DataContext {
            current_token: token,
            piggyback: None,
            tokens: Default::default(),
            values: Default::default(),
            suspended_tokens: Default::default(),
            folded_contexts: Default::default(),
            folded_values: Default::default(),
            imported_tags: Default::default(),
        }
    }

    fn record_token(&mut self, vid: Vid) {
        self.tokens
            .insert_or_error(vid, self.current_token.clone())
            .unwrap();
    }

    fn activate_token(self, vid: &Vid) -> DataContext<Vertex> {
        DataContext {
            current_token: self.tokens[vid].clone(),
            tokens: self.tokens,
            values: self.values,
            suspended_tokens: self.suspended_tokens,
            folded_contexts: self.folded_contexts,
            folded_values: self.folded_values,
            piggyback: self.piggyback,
            imported_tags: self.imported_tags,
        }
    }

    fn split_and_move_to_token(&self, new_token: Option<Vertex>) -> DataContext<Vertex> {
        DataContext {
            current_token: new_token,
            tokens: self.tokens.clone(),
            values: self.values.clone(),
            suspended_tokens: self.suspended_tokens.clone(),
            folded_contexts: self.folded_contexts.clone(),
            folded_values: self.folded_values.clone(),
            piggyback: None,
            imported_tags: self.imported_tags.clone(),
        }
    }

    fn move_to_token(self, new_token: Option<Vertex>) -> DataContext<Vertex> {
        DataContext {
            current_token: new_token,
            tokens: self.tokens,
            values: self.values,
            suspended_tokens: self.suspended_tokens,
            folded_contexts: self.folded_contexts,
            folded_values: self.folded_values,
            piggyback: self.piggyback,
            imported_tags: self.imported_tags,
        }
    }

    fn ensure_suspended(mut self) -> DataContext<Vertex> {
        if let Some(token) = self.current_token {
            self.suspended_tokens.push(Some(token));
            DataContext {
                current_token: None,
                tokens: self.tokens,
                values: self.values,
                suspended_tokens: self.suspended_tokens,
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
        match self.current_token {
            None => {
                let current_token = self.suspended_tokens.pop().unwrap();
                DataContext {
                    current_token,
                    tokens: self.tokens,
                    values: self.values,
                    suspended_tokens: self.suspended_tokens,
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
        self.current_token == other.current_token
            && self.tokens == other.tokens
            && self.values == other.values
            && self.suspended_tokens == other.suspended_tokens
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

pub trait Adapter<'vertex> {
    type Vertex: Clone + Debug + 'vertex;

    fn resolve_starting_vertices(
        &mut self,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> VertexIterator<'vertex, Self::Vertex>;

    fn resolve_property(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, FieldValue>;

    #[allow(clippy::too_many_arguments)]
    fn resolve_neighbors(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, VertexIterator<'vertex, Self::Vertex>>;

    fn resolve_coercion(
        &mut self,
        contexts: ContextIterator<'vertex, Self::Vertex>,
        type_name: Arc<str>,
        coerce_to_type: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> ContextOutcomeIterator<'vertex, Self::Vertex, bool>;
}
