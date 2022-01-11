use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum FrontendError {
    #[error("Query failed to parse.")]
    ParseError(#[from] crate::graphql_query::error::ParseError),

    #[error("Filter on property name \"{0}\" uses undefined tag: %{1}")]
    UndefinedTagInFilter(String, String),

    #[error("Multiple fields are being output under the same name: {0:?}")]
    MultipleOutputsWithSameName(DuplicatedNamesConflict),

    #[error("Multiple fields have @tag directives with the same name: {0:?}")]
    MultipleTagsWithSameName(DuplicatedNamesConflict),

    #[error("Found an edge with an @output directive, this is not supported: {0}")]
    UnsupportedEdgeOutput(String),

    #[error("Found an edge with an unsupported @filter directive: {0}")]
    UnsupportedEdgeFilter(String),

    #[error("Found an unsupported {1} directive on an edge with @fold: {0}")]
    UnsupportedDirectiveOnFoldedEdge(String, String),

    #[error("Missing required edge parameter {0} on edge {1}")]
    MissingRequiredEdgeParameter(String, String),

    #[error("The query failed to validate against the schema.")]
    ValidationError(#[from] ValidationError),

    #[error("Unexpected error: {0}")]
    OtherError(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct DuplicatedNamesConflict {
    // duplicate output name -> vec (type name, field name) being output under that name
    // TODO: it may be better to replace the type name with the edge used to get to the type,
    //       and ideally also add span / Pos info pointing to the specific thing that's the problem.
    pub duplicates: BTreeMap<String, Vec<(String, String)>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum ValidationError {
    #[error("The referenced path does not exist in the schema: {0:?}")]
    NonExistentPath(Vec<String>),

    #[error("The referenced type does not exist in the schema: {0}")]
    NonExistentType(String),

    #[error(
        "Attempted to coerce type {0} into type {1}, but type {0} is not an interface. \
        Only interface types may be coerced to subtypes."
    )]
    CannotCoerceNonInterfaceType(String, String),

    #[error(
        "Attempted to coerce type {0} into type {1}, which is not a subtype of {0}. \
        This is not allowed."
    )]
    CannotCoerceToUnrelatedType(String, String),
}

impl From<async_graphql_parser::Error> for FrontendError {
    fn from(e: async_graphql_parser::Error) -> Self {
        Self::ParseError(e.into())
    }
}

// HACK: necessary to minimize immediate breakage,
//       should be removed as more refined error variants are added
impl From<&str> for FrontendError {
    fn from(s: &str) -> Self {
        Self::OtherError(s.to_owned())
    }
}
impl From<String> for FrontendError {
    fn from(s: String) -> Self {
        Self::OtherError(s)
    }
}
