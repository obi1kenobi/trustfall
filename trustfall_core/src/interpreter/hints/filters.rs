use std::{borrow::Cow, collections::BTreeMap, fmt::Debug, ops::Bound, sync::Arc};

use itertools::Itertools;

use crate::ir::{Argument, FieldRef, FieldValue, FoldSpecificFieldKind, IRFold, Operation};

use super::{candidates::NullableValue, CandidateValue, Range};

pub(super) fn candidate_from_statically_evaluated_filters<'a, 'b, T: Debug + Clone + Eq + 'a>(
    relevant_filters: impl Iterator<Item = &'a Operation<T, Argument>>,
    query_variables: &'b BTreeMap<Arc<str>, FieldValue>,
    is_subject_field_nullable: bool, // whether the field being filtered is nullable in the schema
) -> Option<CandidateValue<Cow<'b, FieldValue>>> {
    let (candidates, post_processing_filters): (Vec<_>, Vec<&Operation<_, _>>) = relevant_filters
        .partition_map(|op| {
            let argument_value = op.right().and_then(|r| r.evaluate_statically(query_variables));
            match (op, argument_value) {
                (Operation::IsNull(..), _) => {
                    itertools::Either::Left(CandidateValue::Single(Cow::Owned(FieldValue::NULL)))
                }
                (Operation::IsNotNull(..), _) => {
                    itertools::Either::Left(CandidateValue::Range(Range::full_non_null()))
                }
                (Operation::Equals(_, _), Some(value)) => {
                    itertools::Either::Left(CandidateValue::Single(value))
                }
                (Operation::LessThan(_, _), Some(value)) => itertools::Either::Left(
                    CandidateValue::Range(Range::with_end(Bound::Excluded(value), true)),
                ),
                (Operation::LessThanOrEqual(_, _), Some(value)) => itertools::Either::Left(
                    CandidateValue::Range(Range::with_end(Bound::Included(value), true)),
                ),
                (Operation::GreaterThan(_, _), Some(value)) => itertools::Either::Left(
                    CandidateValue::Range(Range::with_start(Bound::Excluded(value), true)),
                ),
                (Operation::GreaterThanOrEqual(_, _), Some(value)) => itertools::Either::Left(
                    CandidateValue::Range(Range::with_start(Bound::Included(value), true)),
                ),
                (Operation::OneOf(_, _), Some(value)) => {
                    itertools::Either::Left(CandidateValue::Multiple(
                        value
                            .as_ref()
                            .as_vec_with(|x| Some(Cow::Owned(x.to_owned())))
                            .expect("query variable was not list-typed"),
                    ))
                }
                (Operation::NotEquals(_, _), Some(value)) if value.is_null() => {
                    // Special case: `!= null` can generate candidates;
                    // it's the only `!=` operand for which this is true.
                    itertools::Either::Left(CandidateValue::Range(Range::full_non_null()))
                }
                _ => itertools::Either::Right(op),
            }
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
        candidates.into_iter().fold(initial_candidate, |mut acc, e| {
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
        .filter_map(|(op, value)| -> Option<Box<dyn Iterator<Item = &FieldValue>>> {
            match op {
                Operation::NotEquals(..) => Some(Box::new(std::iter::once(*value))),
                Operation::NotOneOf(..) => Some(Box::new(
                    value.as_slice().expect("not_one_of operand was not a list").iter(),
                )),
                _ => None,
            }
        })
        .flatten();
    for disallowed in disallowed_values {
        candidate.exclude_single_value(disallowed);
    }

    // TODO: use the other kinds of filters to exclude more candidate values

    Some(candidate)
}

pub(super) fn fold_requires_at_least_one_element(
    query_variables: &BTreeMap<Arc<str>, FieldValue>,
    fold: &IRFold,
) -> bool {
    // TODO: When we support applying `@transform` to property-like values, we can update this logic
    //       to be smarter and less conservative.
    let relevant_filters = fold.post_filters.iter().filter(|op| {
        matches!(op.left(), FieldRef::FoldSpecificField(f) if f.kind == FoldSpecificFieldKind::Count)
    });
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
