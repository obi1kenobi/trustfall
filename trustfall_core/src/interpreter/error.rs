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
/// Currently this only wraps errors reported by a
/// [`FallibleAdapter`](crate::interpreter::FallibleAdapter)
/// being queried. It is `#[non_exhaustive]` because interpreter-detected contract violations
/// (today expressed as panics) are expected to migrate into dedicated variants later.
///
/// Execution is fail-fast: the first error an adapter reports terminates the results stream.
/// The in-flight partial result is discarded, exactly one `Err` is yielded, and the stream ends.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExecutionError<E: std::error::Error + 'static> {
    /// The adapter being queried reported an error while resolving a property, edge,
    /// coercion, or starting vertices.
    #[error("The adapter reported an error while executing the query: {0}")]
    Adapter(#[source] E),
}

/// Fallible query results whose error type is uninhabited, resolved to their rows.
///
/// Adapters that set [`Adapter::Error`](crate::interpreter::Adapter::Error) to
/// [`std::convert::Infallible`] can never fail, so their execution results can be
/// unwrapped without ceremony — this trait performs that unwrap in the type system,
/// keeping "infallible adapters never write `Result`, `Ok`, or `unwrap`" true on the
/// consumer side as well.
///
/// ```
/// # use std::collections::BTreeMap;
/// # use trustfall_core::interpreter::error::{ExecutionError, IntoRow};
/// let result: Result<BTreeMap<&str, i64>, ExecutionError<std::convert::Infallible>> =
/// #     Ok(BTreeMap::from([("value", 42)]));
/// # let result = result.map(|row| row.into_iter().map(|(k, v)| (k.to_string(), v)).collect());
/// # let result: Result<BTreeMap<String, i64>, _> = result;
/// // instead of `.expect("infallible adapter")`:
/// let row = result.into_row();
/// # assert_eq!(row["value"], 42);
/// ```
pub trait IntoRow: Sized {
    /// The successful value carried by this result.
    type Row;

    /// Consume this result, returning its row. Panics are impossible by construction:
    /// the error type is uninhabited.
    fn into_row(self) -> Self::Row;
}

impl<Row> IntoRow for Result<Row, ExecutionError<std::convert::Infallible>> {
    type Row = Row;

    fn into_row(self) -> Row {
        match self {
            Ok(row) => row,
            Err(ExecutionError::Adapter(unreachable)) => match unreachable {},
        }
    }
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
