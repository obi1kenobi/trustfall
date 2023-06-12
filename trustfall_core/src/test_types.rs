use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    frontend::error::FrontendError,
    graphql_query::{error::ParseError, query::Query},
    interpreter::trace::Trace,
    ir::{FieldValue, IRQuery, Output},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestGraphQLQuery {
    pub schema_name: String,

    pub query: String,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub arguments: BTreeMap<String, FieldValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestParsedGraphQLQuery {
    pub schema_name: String,

    pub query: Query,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub arguments: BTreeMap<String, FieldValue>,
}

pub type TestParsedGraphQLQueryResult = Result<TestParsedGraphQLQuery, ParseError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestIRQuery {
    pub schema_name: String,

    pub ir_query: IRQuery,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub arguments: BTreeMap<String, FieldValue>,
}

pub type TestIRQueryResult = Result<TestIRQuery, FrontendError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound = "Vertex: Serialize + DeserializeOwned")]
pub struct TestInterpreterOutputTrace<Vertex>
where
    Vertex: Clone + Debug + PartialEq + Eq + Serialize + DeserializeOwned,
{
    pub schema_name: String,

    pub trace: Trace<Vertex>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestInterpreterOutputData {
    pub schema_name: String,

    pub outputs: BTreeMap<Arc<str>, Output>,

    pub results: Vec<BTreeMap<Arc<str>, FieldValue>>,
}
