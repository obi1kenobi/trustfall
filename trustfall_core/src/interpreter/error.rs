use serde::{Deserialize, Serialize};

use crate::{ir::FieldValue, util::DisplayVec};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum QueryArgumentsError {
    #[error("One or more arguments required by this query were not provided: {0:?}")]
    MissingArguments(Vec<String>),

    #[error("One or more of the provided arguments are not used in this query: {0:?}")]
    UnusedArguments(Vec<String>),

    #[error(
        "The query requires argument \"{0}\" to have type {1}, but the provided value cannot be \
        converted to that type: {2:?}"
    )]
    ArgumentTypeError(String, String, FieldValue),

    #[error("Multiple argument errors: {0}")]
    MultipleErrors(DisplayVec<QueryArgumentsError>),
}

impl From<Vec<QueryArgumentsError>> for QueryArgumentsError {
    fn from(v: Vec<QueryArgumentsError>) -> Self {
        assert!(!v.is_empty());
        if v.len() == 1 {
            v.into_iter().next().unwrap()
        } else {
            Self::MultipleErrors(DisplayVec(v))
        }
    }
}
