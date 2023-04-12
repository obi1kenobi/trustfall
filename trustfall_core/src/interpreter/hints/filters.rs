use std::{collections::BTreeMap, fmt::Debug, ops::Bound, sync::Arc};

use itertools::Itertools;

use crate::ir::{Argument, FieldValue, FoldSpecificFieldKind, IRFold, Operation};

use super::{candidates::NullableValue, CandidateValue, Range};

pub(super) fn candidate_from_statically_evaluated_filters<'a, 'b, T: Debug + Clone + Eq + 'a>(
    relevant_filters: impl Iterator<Item = &'a Operation<T, Argument>>,
    query_variables: &'b BTreeMap<Arc<str>, FieldValue>,
    is_subject_field_nullable: bool, // whether the field being filtered is nullable in the schema
) -> Option<CandidateValue<&'b FieldValue>> {
    let (candidates, post_processing_filters): (Vec<_>, Vec<&Operation<_, _>>) = relevant_filters
        .partition_map(|op| match op {
            Operation::IsNull(..) => {
                itertools::Either::Left(CandidateValue::Single(&FieldValue::NULL))
            }
            Operation::IsNotNull(..) => {
                itertools::Either::Left(CandidateValue::Range(Range::full_non_null()))
            }
            Operation::Equals(_, Argument::Variable(var)) => itertools::Either::Left(
                CandidateValue::Single(&query_variables[var.variable_name.as_ref()]),
            ),
            Operation::LessThan(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_end(
                    Bound::Excluded(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::LessThanOrEqual(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_end(
                    Bound::Included(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::GreaterThan(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_start(
                    Bound::Excluded(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::GreaterThanOrEqual(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Range(Range::with_start(
                    Bound::Included(&query_variables[var.variable_name.as_ref()]),
                    true,
                )))
            }
            Operation::OneOf(_, Argument::Variable(var)) => {
                itertools::Either::Left(CandidateValue::Multiple(
                    query_variables[var.variable_name.as_ref()]
                        .as_vec(Option::Some)
                        .expect("query variable was not list-typed"),
                ))
            }
            Operation::NotEquals(_, Argument::Variable(var))
                if query_variables[var.variable_name.as_ref()].is_null() =>
            {
                // Special case: `!= null` can generate candidates;
                // it's the only `!=` operand for which this is true.
                itertools::Either::Left(CandidateValue::Range(Range::full_non_null()))
            }
            _ => itertools::Either::Right(op),
        });

    let mut candidate = if candidates.is_empty() {
        // No valid candidate-producing filters found.
        return None;
    } else {
        let initial_candidate = if is_subject_field_nullable {
            CandidateValue::All
        } else {
            CandidateValue::Range(Range::full_non_null())
        };
        candidates
            .into_iter()
            .fold(initial_candidate, |mut acc, e| {
                acc.intersect(e);
                acc
            })
    };
    candidate.normalize();

    // If we have any filters that may affect the candidate value in post-processing,
    // get their operand values now.
    if post_processing_filters.is_empty() {
        return Some(candidate);
    }
    let filters_and_values: Vec<_> = post_processing_filters
        .into_iter()
        .filter_map(|op| {
            op.right().and_then(|arg| {
                if let Argument::Variable(var) = arg {
                    Some((op, &query_variables[var.variable_name.as_ref()]))
                } else {
                    None
                }
            })
        })
        .collect();

    // Ensure the candidate isn't any value that is directly disallowed by
    // a `!=` or equivalent filter operation.
    let disallowed_values = filters_and_values
        .iter()
        .filter_map(
            |(op, value)| -> Option<Box<dyn Iterator<Item = &FieldValue>>> {
                match op {
                    Operation::NotEquals(..) => Some(Box::new(std::iter::once(*value))),
                    Operation::NotOneOf(..) => Some(Box::new(
                        value
                            .as_slice()
                            .expect("not_one_of operand was not a list")
                            .iter(),
                    )),
                    _ => None,
                }
            },
        )
        .flatten();
    for disallowed in disallowed_values {
        candidate.exclude_single_value(&disallowed);
    }

    // TODO: use the other kinds of filters to exclude more candidate values

    Some(candidate)
}

pub(super) fn fold_requires_at_least_one_element(
    query_variables: &BTreeMap<Arc<str>, FieldValue>,
    fold: &IRFold,
) -> bool {
    let relevant_filters = fold
        .post_filters
        .iter()
        .filter(|op| matches!(op.left(), FoldSpecificFieldKind::Count));
    let is_subject_field_nullable = false; // the "count" value can't be null
    candidate_from_statically_evaluated_filters(
        relevant_filters,
        query_variables,
        is_subject_field_nullable,
    )
    .map(|value| {
        // Ensure all candidate values require the count to be >= 1.
        match value {
            CandidateValue::Impossible => false,
            CandidateValue::Single(x) => x.as_u64().unwrap_or_default() >= 1,
            CandidateValue::Multiple(multi) => {
                multi.iter().all(|x| x.as_u64().unwrap_or_default() >= 1)
            }
            CandidateValue::Range(r) => match r.start_bound() {
                Bound::Included(inc) => inc.as_u64().unwrap_or_default() >= 1,
                Bound::Excluded(inc) => inc.as_u64().is_some(), // any u64 is >= 0
                Bound::Unbounded => false,
            },
            CandidateValue::All => false,
        }
    })
    .unwrap_or(false)
}
