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

/// An error surfaced while *executing* a query, i.e. while pulling results from the
/// iterator returned by [`interpret_ir`](crate::interpreter::execution::interpret_ir).
///
/// Currently this only wraps errors reported by the adapter being queried. It is
/// `#[non_exhaustive]` because interpreter-detected contract violations (today expressed as
/// panics) are expected to migrate into dedicated variants later.
///
/// Execution is fail-fast: the first error an adapter reports terminates the results stream.
/// The in-flight partial result is discarded, exactly one `Err` is yielded, and the stream ends.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecutionError<E: std::error::Error + 'static> {
    /// The adapter being queried reported an error while resolving a property, edge,
    /// coercion, or starting vertices.
    #[error("the adapter reported an error while executing the query: {0}")]
    Adapter(#[source] E),
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
