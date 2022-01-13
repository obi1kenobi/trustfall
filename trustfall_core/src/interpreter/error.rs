use serde::{Deserialize, Serialize};

use crate::util::DisplayVec;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, thiserror::Error)]
pub enum QueryArgumentsError {
    #[error("One or more arguments required by this query were not provided: {0:?}")]
    MissingArgument(Vec<String>),

    #[error("One or more of the provided arguments are not used in this query: {0:?}")]
    UnusedArgument(Vec<String>),

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
