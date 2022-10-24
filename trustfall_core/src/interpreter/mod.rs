use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use async_graphql_parser::types::Type;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::{
    ir::{
        indexed::IndexedQuery, types::is_argument_type_valid, EdgeParameters, Eid, FieldValue, Vid,
    },
    util::BTreeMapTryInsertExt,
};

use self::error::QueryArgumentsError;

pub mod basic_adapter;
pub mod error;
pub mod execution;
mod filtering;
pub mod macros;
pub mod replay;
pub mod trace;

#[derive(Debug, Clone)]
pub struct DataContext<DataToken: Clone + Debug> {
    pub current_token: Option<DataToken>,
    tokens: BTreeMap<Vid, Option<DataToken>>,
    values: Vec<FieldValue>,
    suspended_tokens: Vec<Option<DataToken>>,
    folded_contexts: BTreeMap<Eid, Vec<DataContext<DataToken>>>,
    folded_values: BTreeMap<(Eid, Arc<str>), ValueOrVec>,
    piggyback: Option<Vec<DataContext<DataToken>>>,
    imported_tags: BTreeMap<(Vid, Arc<str>), FieldValue>,
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
#[serde(bound = "DataToken: Serialize, for<'de2> DataToken: Deserialize<'de2>")]
struct SerializableContext<DataToken>
where
    DataToken: Clone + Debug + Serialize,
    for<'d> DataToken: Deserialize<'d>,
{
    current_token: Option<DataToken>,
    tokens: BTreeMap<Vid, Option<DataToken>>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    values: Vec<FieldValue>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    suspended_tokens: Vec<Option<DataToken>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folded_contexts: BTreeMap<Eid, Vec<DataContext<DataToken>>>,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    folded_values: BTreeMap<(Eid, Arc<str>), ValueOrVec>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    piggyback: Option<Vec<DataContext<DataToken>>>,

    /// Tagged values imported from an ancestor component of the one currently being evaluated.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    imported_tags: BTreeMap<(Vid, Arc<str>), FieldValue>,
}

impl<DataToken> From<SerializableContext<DataToken>> for DataContext<DataToken>
where
    DataToken: Clone + Debug + Serialize,
    for<'d> DataToken: Deserialize<'d>,
{
    fn from(context: SerializableContext<DataToken>) -> Self {
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

impl<DataToken> From<DataContext<DataToken>> for SerializableContext<DataToken>
where
    DataToken: Clone + Debug + Serialize,
    for<'d> DataToken: Deserialize<'d>,
{
    fn from(context: DataContext<DataToken>) -> Self {
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

impl<DataToken: Clone + Debug> DataContext<DataToken> {
    fn new(token: Option<DataToken>) -> DataContext<DataToken> {
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

    fn activate_token(self, vid: &Vid) -> DataContext<DataToken> {
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

    fn split_and_move_to_token(&self, new_token: Option<DataToken>) -> DataContext<DataToken> {
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

    fn move_to_token(self, new_token: Option<DataToken>) -> DataContext<DataToken> {
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

    fn ensure_suspended(mut self) -> DataContext<DataToken> {
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

    fn ensure_unsuspended(mut self) -> DataContext<DataToken> {
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

impl<DataToken: Debug + Clone + PartialEq> PartialEq for DataContext<DataToken> {
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

impl<DataToken: Debug + Clone + PartialEq + Eq> Eq for DataContext<DataToken> {}

impl<DataToken> Serialize for DataContext<DataToken>
where
    DataToken: Debug + Clone + Serialize,
    for<'d> DataToken: Deserialize<'d>,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // TODO: eventually maybe write a proper (de)serialize?
        SerializableContext::from(self.clone()).serialize(serializer)
    }
}

impl<'de, DataToken> Deserialize<'de> for DataContext<DataToken>
where
    DataToken: Debug + Clone + Serialize,
    for<'d> DataToken: Deserialize<'d>,
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

pub trait Adapter<'token> {
    type DataToken: Clone + Debug + 'token;

    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'token>;

    #[allow(clippy::type_complexity)]
    fn project_property(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: Arc<str>,
        field_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, FieldValue)> + 'token>;

    #[allow(clippy::type_complexity)]
    #[allow(clippy::too_many_arguments)]
    fn project_neighbors(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: Arc<str>,
        edge_name: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
        edge_hint: Eid,
    ) -> Box<
        dyn Iterator<
                Item = (
                    DataContext<Self::DataToken>,
                    Box<dyn Iterator<Item = Self::DataToken> + 'token>,
                ),
            > + 'token,
    >;

    fn can_coerce_to_type(
        &mut self,
        data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
        current_type_name: Arc<str>,
        coerce_to_type_name: Arc<str>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = (DataContext<Self::DataToken>, bool)> + 'token>;
}
