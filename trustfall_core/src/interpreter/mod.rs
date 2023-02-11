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
    folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>>,
    piggyback: Option<Vec<DataContext<DataToken>>>,
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
    folded_values: BTreeMap<(Eid, Arc<str>), Option<ValueOrVec>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    piggyback: Option<Vec<DataContext<DataToken>>>,

    /// Tagged values imported from an ancestor component of the one currently being evaluated.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    imported_tags: BTreeMap<FieldRef, FieldValue>,
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
    pub fn new(token: Option<DataToken>) -> DataContext<DataToken> {
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

/// An `Adapter` is an entry point between the Trustfall query engine and an
/// external API, file, database or whatever.
///
/// By providing Trustfall with ways of finding and relating the `DataToken`s
/// for your particular data source, it can be queried together with other data
/// sources.
///
/// Although `DataToken` may be anything within the constraints, normally it
/// would be on the form
///
/// ```
/// # use std::rc::Rc;
/// #[derive(Debug, Clone)]
/// struct Student {
///     name: String,
///     homework: Vec<Homework>,
/// };
///
/// #[derive(Debug, Clone)]
/// struct Homework;
///
/// #[derive(Debug, Clone)]
/// enum DataToken {
///     StudentToken(Rc<Student>),
///     HomeworkToken(Rc<Homework>),
/// }
/// ```
pub trait Adapter<'token> {
    type DataToken: Clone + Debug + 'token;

    /// Retrieves an iterator of `DataToken`s from an entry point for this
    /// adapter based on the name of the entry point and which parameters is
    /// passed to it.
    ///
    /// Arguments:
    /// * `edge`: The name of the query field as a string
    /// * `parameters`: Arguments passed to the field
    /// * `query_hint`: An optional already interpreted and indexed query for
    ///                 some arguments to speed up the query
    /// * `vertex_hint`:
    ///
    /// In GraphQL, this would be correspond to all fields the _Root_ type or
    /// the _Query_ type. In the following GraphQL schema, `student` is a field
    /// of the Query type, but `homework` and `name` are not. This means that
    /// while `student` is a starting token, `homework` and `name` are not.
    /// ```graphql
    /// type Query {
    ///     student(name: String!): Student!
    /// }
    ///
    /// type Student {
    ///     name: String!
    ///     homework: [Homework]!
    /// }
    ///
    /// // ...
    /// ```
    ///
    /// In this example, `edge` would be `"student"`, `parameters` would be be a
    /// `BTreeMap` containing a mapping `name` to some [FieldValue::String]
    /// value. The returned value would be an iterator over a single `Student`-like
    /// `DataToken`.
    fn get_starting_tokens(
        &mut self,
        edge: Arc<str>,
        parameters: Option<Arc<EdgeParameters>>,
        query_hint: InterpretedQuery,
        vertex_hint: Vid,
    ) -> Box<dyn Iterator<Item = Self::DataToken> + 'token>;

    /// Implement a property on all `DataTokens` in an iterator of contexts,
    /// returning an iterator over tuples of context-value pairs.
    ///
    /// Arguments:
    /// * `data_contexts`: Tokens and their contexts, such as other known tokens
    /// * `current_type_name`: The name of the current type
    /// * `field_name`: The name of the field having this property
    /// * `query_hint`
    /// * `vertex_hint`
    ///
    /// In GraphQL this would correspond to retrieving a field from another
    /// field.
    /// ```graphql
    /// type Student {
    ///     name: String!
    ///     homework: [Homework]!
    /// }
    /// ```
    ///
    /// Using the schema above, to retrieve the name of a `"student"`
    /// `DataToken` would require a call like the following, assuming `ctx`
    /// contains a `DataToken`.
    ///
    /// ```ignore
    /// project_property(ctx, "Student", "name", query_hint, vertex_hint)
    /// ```
    ///
    /// Normally implementing this requires a lot of repetition as all
    /// properties are added to the types that contains them (like all GraphQL
    /// fields with a property called `"name"` of type `String`). This can be be
    /// made easier by using declarative macros, for example like so
    ///
    /// ```ignore
    /// fn project_property(
    ///    data_contexts: Box<dyn Iterator<Item = DataContext<Self::DataToken>> + 'token>,
    ///    current_type_name: Arc<str>,
    ///    field_name: Arc<str>,
    ///    query_hint: InterpretedQuery,
    ///    vertex_hint: Vid,
    /// ) {
    ///     match ((&current_type_name).as_ref(), &field_name.as_ref()) => {
    ///         ("Student", "name") => impl_property!(data_contexts, as_student, name),
    ///          // ...
    ///     }
    /// }
    /// ```
    ///
    /// where `impl_property!` simply maps over all contexts, creating a new
    /// iterator of ([DataContext], [FieldValue]) by converting to a student
    /// using a `as_student` method implemented on `DataToken` and then
    /// retrieving the `name` attribute of it.
    ///
    /// This relies on that the `DataToken` type all implement `as_student`,
    /// returning an `Option` if the conversion succeeded, thus ignoring all
    /// tokens that can not be converted to a student (in this example, probably
    /// a `Homework` `DataToken`) and setting the value of their `name` as a
    /// [FieldValue::Null].
    ///
    /// A simple example of this is the following taken from the `hackernews`
    /// demo in the `trustfall` repository:
    ///
    /// ```
    /// macro_rules! impl_property {
    ///     ($data_contexts:ident, $conversion:ident, $attr:ident) => {
    ///         Box::new($data_contexts.map(|ctx| {
    ///             let token = ctx
    ///                 .current_token
    ///                 .as_ref()
    ///                 .map(|token| token.$conversion().unwrap());
    ///             let value = match token {
    ///                 None => FieldValue::Null,
    ///                 Some(t) => (&t.$attr).into(),
    ///                 #[allow(unreachable_patterns)]
    ///                 _ => unreachable!(),
    ///             };
    ///
    ///             (ctx, value)
    ///         }))
    ///     };
    /// }
    /// ```
    ///
    /// which in our case would be expanded to (here with type annotations)
    /// ```
    /// # use trustfall_core::{interpreter::DataContext, ir::FieldValue};
    /// # use std::rc::Rc;
    /// # #[derive(Debug, Clone)]
    /// # struct Student {
    /// #     name: String,
    /// #     homework: Vec<Homework>,
    /// # };
    /// # #[derive(Debug, Clone)]
    /// # struct Homework;
    /// # #[derive(Debug, Clone)]
    /// # enum DataToken {
    /// #     StudentToken(Rc<Student>),
    /// #     HomeworkToken(Rc<Homework>),
    /// # }
    ///
    /// impl DataToken {
    ///     pub fn as_student(&self) -> Option<&Student> {
    ///         match self {
    ///             DataToken::StudentToken(s) => Some(s.as_ref()),
    ///             _ => None,
    ///         }
    ///     }
    /// }
    ///
    /// // ...
    ///
    /// # fn expanded(data_contexts: Box<dyn Iterator<Item = DataContext<DataToken>>>)
    /// # -> Box<dyn Iterator<Item = (DataContext<DataToken>, FieldValue)>> {
    /// Box::new(data_contexts.map(|ctx| {
    ///     let stud: Option<&Student> = (&ctx
    ///         .current_token)       // Option<Token>
    ///         .as_ref()             // Option<&Token>
    ///         .map(|t: &DataToken| {
    ///             t                 // &Token
    ///                 .as_student() // Option<&Student>
    ///                 .unwrap()     // Option<Option<&Student> => Option<&Student>
    ///         });
    ///
    ///     let value: FieldValue = match stud {
    ///         None => FieldValue::Null,
    ///         Some(s) => (&s.name).into(),
    ///     };
    ///
    ///     (ctx, value)
    /// }))
    /// # }
    /// ```
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
