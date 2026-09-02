//! Synchronous query execution.
//!
//! Trustfall has one execution kernel, built from runtime-agnostic streams. This
//! module projects that kernel as the traditional synchronous API. The projection
//! does not buffer, collect, or split adapter batches: [`SyncAdapter`] turns each
//! ready stream into a lazy iterator and [`ReadyIterator`] exposes the result stream
//! the same way.

use std::{collections::BTreeMap, sync::Arc};

use crate::ir::{Argument, FieldValue, FoldSpecificFieldKind, IRFold, IndexedQuery, Operation};

use super::{
    Adapter, InterpretedQuery,
    engine::interpret_ir as interpret_stream,
    error::{ExecutionError, QueryArgumentsError},
    sync_adapter::{ReadyIterator, SyncAdapter},
};

#[derive(Debug, Clone)]
pub(super) struct QueryCarrier {
    pub(in crate::interpreter) query: Option<InterpretedQuery>,
}

/// Execute an indexed query synchronously.
///
/// Query validation happens eagerly. Data resolution remains lazy and fail-fast:
/// consuming the returned iterator drives the shared stream kernel until either a
/// row or the first adapter error is produced.
#[allow(clippy::type_complexity)]
pub fn interpret_ir<'query, A: Adapter<'query> + 'query>(
    adapter: Arc<A>,
    indexed_query: Arc<IndexedQuery>,
    arguments: Arc<BTreeMap<Arc<str>, FieldValue>>,
) -> Result<
    Box<
        dyn Iterator<Item = Result<BTreeMap<Arc<str>, FieldValue>, ExecutionError<A::Error>>>
            + 'query,
    >,
    QueryArgumentsError,
> {
    let adapter = Arc::new(SyncAdapter::new(adapter));
    let stream = interpret_stream(adapter, indexed_query, arguments)?;
    Ok(Box::new(ReadyIterator::new(stream)))
}

fn usize_from_field_value(field_value: &FieldValue) -> Option<usize> {
    match field_value {
        FieldValue::Int64(value) => {
            Some(usize::try_from((*value).max(0)).expect("nonnegative i64 fits in usize"))
        }
        FieldValue::Uint64(value) => {
            Some(usize::try_from(*value).expect("fold count argument fits in usize"))
        }
        FieldValue::Null => None,
        _ => panic!(
            "fold count filter had non-integer value {field_value:#?}; validation should have rejected it"
        ),
    }
}

/// Return the tightest statically-known upper bound on a fold's element count.
pub(super) fn get_max_fold_count_limit(carrier: &QueryCarrier, fold: &IRFold) -> Option<usize> {
    let arguments = &carrier.query.as_ref().expect("query was not returned").arguments;
    fold.post_filters
        .iter()
        .filter_map(|filter| match filter {
            Operation::Equals(FoldSpecificFieldKind::Count, Argument::Variable(variable))
            | Operation::LessThanOrEqual(
                FoldSpecificFieldKind::Count,
                Argument::Variable(variable),
            ) => usize_from_field_value(&arguments[&variable.variable_name]),
            Operation::LessThan(FoldSpecificFieldKind::Count, Argument::Variable(variable)) => {
                usize_from_field_value(&arguments[&variable.variable_name])
                    .map(|value| value.saturating_sub(1))
            }
            Operation::OneOf(FoldSpecificFieldKind::Count, Argument::Variable(variable)) => {
                arguments[&variable.variable_name]
                    .as_slice()
                    .expect("fold count one_of argument was not a list")
                    .iter()
                    .filter_map(usize_from_field_value)
                    .max()
            }
            _ => None,
        })
        .min()
}

/// Return the tightest statically-known lower bound on a fold's element count.
pub(super) fn get_min_fold_count_limit(carrier: &QueryCarrier, fold: &IRFold) -> Option<usize> {
    let arguments = &carrier.query.as_ref().expect("query was not returned").arguments;
    fold.post_filters
        .iter()
        .try_fold(None, |limit, filter| {
            let next = match filter {
                Operation::GreaterThanOrEqual(
                    FoldSpecificFieldKind::Count,
                    Argument::Variable(variable),
                ) => usize_from_field_value(&arguments[&variable.variable_name])?,
                Operation::GreaterThan(
                    FoldSpecificFieldKind::Count,
                    Argument::Variable(variable),
                ) => usize_from_field_value(&arguments[&variable.variable_name])?.saturating_add(1),
                _ => return None,
            };
            Some(Some(limit.map_or(next, |current: usize| current.max(next))))
        })
        .flatten()
}
