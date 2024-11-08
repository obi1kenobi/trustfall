use std::{fmt::Debug, mem};

use regex::Regex;

use crate::ir::{Argument, FieldRef, FieldValue, IRQueryComponent, LocalField, Operation, Vid};

use super::{
    execution::{
        compute_context_field_with_separate_value, compute_fold_specific_field_with_separate_value,
        compute_local_field_with_separate_value, QueryCarrier,
    },
    Adapter, ContextIterator, ContextOutcomeIterator, DataContext, TaggedValue,
};

#[inline(always)]
pub(super) fn equals(left: &FieldValue, right: &FieldValue) -> bool {
    if mem::discriminant(left) == mem::discriminant(right) {
        match (left, right) {
            (FieldValue::List(l), FieldValue::List(r)) => {
                l.len() == r.len() && l.iter().zip(r.iter()).all(|(x, y)| equals(x, y))
            }
            _ => left == right,
        }
    } else {
        match (left, right) {
            (&FieldValue::Uint64(l), &FieldValue::Int64(r)) => {
                if let Ok(l) = i64::try_from(l) {
                    l == r
                } else if let Ok(r) = u64::try_from(r) {
                    l == r
                } else {
                    false
                }
            }
            (&FieldValue::Int64(l), &FieldValue::Uint64(r)) => {
                if let Ok(l) = u64::try_from(l) {
                    l == r
                } else if let Ok(r) = i64::try_from(r) {
                    l == r
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

macro_rules! make_comparison_op_func {
    ( $func: ident, $op: tt, $slow_path_handler: ident ) => {
        #[inline(always)]
        pub(super) fn $func(left: &FieldValue, right: &FieldValue) -> bool {
            match (left, right) {
                (FieldValue::Null, _) => false,
                (_, FieldValue::Null) => false,
                (FieldValue::String(l), FieldValue::String(r)) => l $op r,
                (FieldValue::Int64(l), FieldValue::Int64(r)) => l $op r,
                (FieldValue::Uint64(l), FieldValue::Uint64(r)) => l $op r,
                (FieldValue::Float64(l), FieldValue::Float64(r)) => l $op r,
                _ => $slow_path_handler(left, right),
            }
        }
    };
}

macro_rules! make_greater_than_func_slow_path {
    ( $func: ident, $op: tt) => {
        #[inline(always)]
        fn $func(left: &FieldValue, right: &FieldValue) -> bool {
            match (left, right) {
                (&FieldValue::Int64(l), &FieldValue::Uint64(r)) => {
                    if let Ok(l) = u64::try_from(l) {
                        l $op r
                    } else if let Ok(r) = i64::try_from(r) {
                        l $op r
                    } else if l < 0 {
                        false
                    } else {
                        unreachable!("values {:?} and {:?}", left, right)
                    }
                }
                (&FieldValue::Uint64(l), &FieldValue::Int64(r)) => {
                    if let Ok(l) = i64::try_from(l) {
                        l $op r
                    } else if let Ok(r) = u64::try_from(r) {
                        l $op r
                    } else if r < 0 {
                        true
                    } else {
                        unreachable!("values {:?} and {:?}", left, right)
                    }
                }
                _ => unreachable!("values {:?} and {:?}", left, right)
            }
        }
    };
}

macro_rules! make_less_than_func_slow_path {
    ( $func: ident, $op: tt) => {
        #[inline(always)]
        fn $func(left: &FieldValue, right: &FieldValue) -> bool {
            match (left, right) {
                (&FieldValue::Int64(l), &FieldValue::Uint64(r)) => {
                    if let Ok(l) = u64::try_from(l) {
                        l $op r
                    } else if let Ok(r) = i64::try_from(r) {
                        l $op r
                    } else if l < 0 {
                        true
                    } else {
                        unreachable!("values {:?} and {:?}", left, right)
                    }
                }
                (&FieldValue::Uint64(l), &FieldValue::Int64(r)) => {
                    if let Ok(l) = i64::try_from(l) {
                        l $op r
                    } else if let Ok(r) = u64::try_from(r) {
                        l $op r
                    } else if r < 0 {
                        false
                    } else {
                        unreachable!("values {:?} and {:?}", left, right)
                    }
                }
                _ => unreachable!("values {:?} and {:?}", left, right)
            }
        }
    };
}

make_greater_than_func_slow_path!(slow_path_greater_than, >);
make_comparison_op_func!(greater_than, >, slow_path_greater_than);
make_greater_than_func_slow_path!(slow_path_greater_than_or_equal, >=);
make_comparison_op_func!(greater_than_or_equal, >=, slow_path_greater_than_or_equal);
make_less_than_func_slow_path!(slow_path_less_than, <);
make_comparison_op_func!(less_than, <, slow_path_less_than);
make_less_than_func_slow_path!(slow_path_less_than_or_equal, <=);
make_comparison_op_func!(less_than_or_equal, <=, slow_path_less_than_or_equal);

#[inline(always)]
pub(super) fn has_substring(left: &FieldValue, right: &FieldValue) -> bool {
    match (left, right) {
        (FieldValue::String(l), FieldValue::String(r)) => l.contains(r.as_ref()),
        (FieldValue::Null, FieldValue::String(_))
        | (FieldValue::String(_), FieldValue::Null)
        | (FieldValue::Null, FieldValue::Null) => false,
        _ => unreachable!("{:?} {:?}", left, right),
    }
}

#[inline(always)]
pub(super) fn has_prefix(left: &FieldValue, right: &FieldValue) -> bool {
    match (left, right) {
        (FieldValue::String(l), FieldValue::String(r)) => l.starts_with(r.as_ref()),
        (FieldValue::Null, FieldValue::String(_))
        | (FieldValue::String(_), FieldValue::Null)
        | (FieldValue::Null, FieldValue::Null) => false,
        _ => unreachable!("{:?} {:?}", left, right),
    }
}

#[inline(always)]
pub(super) fn has_suffix(left: &FieldValue, right: &FieldValue) -> bool {
    match (left, right) {
        (FieldValue::String(l), FieldValue::String(r)) => l.ends_with(r.as_ref()),
        (FieldValue::Null, FieldValue::String(_))
        | (FieldValue::String(_), FieldValue::Null)
        | (FieldValue::Null, FieldValue::Null) => false,
        _ => unreachable!("{:?} {:?}", left, right),
    }
}

#[inline(always)]
pub(super) fn one_of(left: &FieldValue, right: &FieldValue) -> bool {
    match right {
        FieldValue::Null => false,
        FieldValue::List(v) => {
            for value in v.iter() {
                if left == value {
                    return true;
                }
            }
            false
        }
        _ => unreachable!("{:?} {:?}", left, right),
    }
}

#[inline(always)]
pub(super) fn contains(left: &FieldValue, right: &FieldValue) -> bool {
    one_of(right, left)
}

/// Implement checking a value against a regex pattern.
///
/// This function should be used when checking a regex filter that uses a tag in the filter,
/// since it will recompile the regex for each check, and this is slow. For regex checks against
/// a runtime parameter, the optimized variant of this function should be called,
/// with a precompiled regex pattern matching the runtime parameter value.
#[inline(always)]
pub(super) fn regex_matches_slow_path(left: &FieldValue, right: &FieldValue) -> bool {
    match (left, right) {
        (FieldValue::String(l), FieldValue::String(r)) => {
            // Bad regex values can happen in ways that can't be prevented,
            // for example: when using a tag argument and the tagged value isn't a valid regex.
            // In such cases, we declare that the regex doesn't match.
            Regex::new(r).map(|pattern| pattern.is_match(l)).unwrap_or(false)
        }
        (FieldValue::Null, FieldValue::Null)
        | (FieldValue::Null, FieldValue::String(_))
        | (FieldValue::String(_), FieldValue::Null) => false,
        _ => unreachable!("{:?} {:?}", left, right),
    }
}

#[inline(always)]
pub(super) fn regex_matches_optimized(left: &FieldValue, regex: &Regex) -> bool {
    match left {
        FieldValue::String(l) => regex.is_match(l),
        FieldValue::Null => false,
        _ => unreachable!("{:?}", left),
    }
}

fn apply_unary_filter<
    'query,
    Vertex: Debug + Clone + 'query,
    FilterFn: Fn(&FieldValue) -> bool + 'query,
>(
    filter_op: FilterFn,
    iterator: ContextIterator<'query, Vertex>,
) -> ContextIterator<'query, Vertex> {
    Box::new(iterator.filter_map(move |mut context| {
        let last_value = context.values.pop().expect("no value present");
        (context.within_nonexistent_optional() || filter_op(&last_value)).then_some(context)
    }))
}

#[inline(always)]
fn is_null(value: &FieldValue) -> bool {
    matches!(value, FieldValue::Null)
}

fn attempt_apply_unary_filter<'query, Vertex: Debug + Clone + 'query>(
    filter: &Operation<(), &Argument>,
    iterator: ContextIterator<'query, Vertex>,
) -> Result<ContextIterator<'query, Vertex>, ContextIterator<'query, Vertex>> {
    match filter {
        Operation::IsNull(_) => Ok(apply_unary_filter(is_null, iterator)),
        Operation::IsNotNull(_) => Ok(apply_unary_filter(|v| !is_null(v), iterator)),
        _ => Err(iterator),
    }
}

pub(super) fn apply_filter<'query, AdapterT: Adapter<'query>>(
    adapter: &AdapterT,
    carrier: &mut QueryCarrier,
    component: &IRQueryComponent,
    current_vid: Vid,
    filter: &Operation<(), &Argument>,
    iterator: ContextIterator<'query, AdapterT::Vertex>,
) -> ContextIterator<'query, AdapterT::Vertex> {
    // If the filter operator is unary, we don't need to evaluate any arguments.
    // Short-circuit it here.
    let iterator = match attempt_apply_unary_filter(filter, iterator) {
        Ok(output) => return output,
        Err(iterator) => iterator,
    };

    // TODO: implement more efficient filtering with:
    //       - type awareness: we know the type of the field being filtered,
    //         and we probably know (or can infer) the type of the filtering argument(s)
    //       - when using tagged values as regexes, adjacent tag values are likely to be equal
    //         due to expansion rules, so keep the previous regex around and reuse if possible
    //         instead of rebuilding
    //       - turn "in_collection" filter arguments into sets if possible
    match filter.right() {
        Some(Argument::Variable(var)) => {
            let query_arguments =
                &carrier.query.as_ref().expect("query was not returned").arguments;
            let right_value = query_arguments[var.variable_name.as_ref()].to_owned();
            apply_filter_with_static_argument_value(filter, right_value, iterator)
        }
        Some(Argument::Tag(FieldRef::ContextField(context_field))) => {
            // TODO: Benchmark if it would be faster to duplicate the filtering code to special-case
            //       the situation when the tag is always known to exist, so we don't have to unwrap
            //       a TaggedValue enum, because we know it would be TaggedValue::Some.
            let argument_value_iterator = if context_field.vertex_id == current_vid {
                // This tag is from the vertex we're currently filtering. That means the field
                // whose value we want to get is actually local, so there's no need to compute it
                // using the more expensive approach we use for non-local fields.
                let local_equivalent_field = LocalField {
                    field_name: context_field.field_name.clone(),
                    field_type: context_field.field_type.clone(),
                };
                Box::new(
                    compute_local_field_with_separate_value(
                        adapter,
                        carrier,
                        component,
                        current_vid,
                        &local_equivalent_field,
                        iterator,
                    )
                    .map(|(ctx, value)| (ctx, TaggedValue::Some(value))),
                )
            } else {
                compute_context_field_with_separate_value(
                    adapter,
                    carrier,
                    component,
                    context_field,
                    iterator,
                )
            };
            apply_filter_with_tagged_argument_value(filter, argument_value_iterator)
        }
        Some(Argument::Tag(field_ref @ FieldRef::FoldSpecificField(fold_field))) => {
            let argument_value_iterator = if component.folds.contains_key(&fold_field.fold_eid) {
                compute_fold_specific_field_with_separate_value(
                    fold_field.fold_eid,
                    &fold_field.kind,
                    iterator,
                )
            } else {
                // This value represents an imported tag value from an outer component.
                // Grab its value from the context itself.
                let cloned_ref = field_ref.clone();
                Box::new(iterator.map(move |ctx| {
                    let right_value = ctx.imported_tags[&cloned_ref].clone();
                    (ctx, right_value)
                }))
            };
            apply_filter_with_tagged_argument_value(filter, argument_value_iterator)
        }
        None => unreachable!(
            "no argument present for filter, but not handled in unary filters fn: {filter:?}"
        ),
    }
}

macro_rules! not {
    ($fn_name:ident) => {
        |l, r| !$fn_name(l, r)
    };
}

fn apply_filter_op_with_static_argument<
    'query,
    RightValue: 'query,
    Vertex: Debug + Clone + 'query,
    FilterFn: Fn(&FieldValue, &RightValue) -> bool + 'query,
>(
    right_value: RightValue,
    filter_op: FilterFn,
    iterator: ContextIterator<'query, Vertex>,
) -> ContextIterator<'query, Vertex> {
    Box::new(iterator.filter_map(move |mut ctx| {
        let left_value = ctx.values.pop().expect("no value present");
        apply_filter_op(ctx, &filter_op, &left_value, &right_value)
    }))
}

fn apply_filter_op_with_tagged_argument<
    'query,
    Vertex: Debug + Clone + 'query,
    FilterFn: Fn(&FieldValue, &FieldValue) -> bool + 'query,
>(
    filter_op: FilterFn,
    iterator: ContextOutcomeIterator<'query, Vertex, TaggedValue>,
) -> ContextIterator<'query, Vertex> {
    Box::new(iterator.filter_map(move |(mut ctx, tagged_value)| {
        let left_value = ctx.values.pop().expect("no value present");
        let TaggedValue::Some(right_value) = tagged_value else {
            return Some(ctx);
        };
        apply_filter_op(ctx, &filter_op, &left_value, &right_value)
    }))
}

#[inline(always)]
fn apply_filter_op<
    'query,
    RightValue: 'query,
    Vertex: Debug + Clone + 'query,
    FilterFn: Fn(&FieldValue, &RightValue) -> bool + 'query,
>(
    ctx: DataContext<Vertex>,
    filter_op: &FilterFn,
    left: &FieldValue,
    right: &RightValue,
) -> Option<DataContext<Vertex>> {
    (ctx.within_nonexistent_optional() || filter_op(left, right)).then_some(ctx)
}

fn apply_filter_with_static_argument_value<'query, Vertex: Debug + Clone + 'query>(
    filter: &Operation<(), &Argument>,
    right_value: FieldValue,
    iterator: ContextIterator<'query, Vertex>,
) -> ContextIterator<'query, Vertex> {
    match filter {
        Operation::Equals(_, _) => {
            apply_filter_op_with_static_argument(right_value, equals, iterator)
        }
        Operation::NotEquals(_, _) => {
            apply_filter_op_with_static_argument(right_value, not!(equals), iterator)
        }
        Operation::LessThan(_, _) => {
            apply_filter_op_with_static_argument(right_value, less_than, iterator)
        }
        Operation::LessThanOrEqual(_, _) => {
            apply_filter_op_with_static_argument(right_value, less_than_or_equal, iterator)
        }
        Operation::GreaterThan(_, _) => {
            apply_filter_op_with_static_argument(right_value, greater_than, iterator)
        }
        Operation::GreaterThanOrEqual(_, _) => {
            apply_filter_op_with_static_argument(right_value, greater_than_or_equal, iterator)
        }
        Operation::Contains(_, _) => {
            apply_filter_op_with_static_argument(right_value, contains, iterator)
        }
        Operation::NotContains(_, _) => {
            apply_filter_op_with_static_argument(right_value, not!(contains), iterator)
        }
        Operation::OneOf(_, _) => {
            apply_filter_op_with_static_argument(right_value, one_of, iterator)
        }
        Operation::NotOneOf(_, _) => {
            apply_filter_op_with_static_argument(right_value, not!(one_of), iterator)
        }
        Operation::HasPrefix(_, _) => {
            apply_filter_op_with_static_argument(right_value, has_prefix, iterator)
        }
        Operation::NotHasPrefix(_, _) => {
            apply_filter_op_with_static_argument(right_value, not!(has_prefix), iterator)
        }
        Operation::HasSuffix(_, _) => {
            apply_filter_op_with_static_argument(right_value, has_suffix, iterator)
        }
        Operation::NotHasSuffix(_, _) => {
            apply_filter_op_with_static_argument(right_value, not!(has_suffix), iterator)
        }
        Operation::HasSubstring(_, _) => {
            apply_filter_op_with_static_argument(right_value, has_substring, iterator)
        }
        Operation::NotHasSubstring(_, _) => {
            apply_filter_op_with_static_argument(right_value, not!(has_substring), iterator)
        }
        Operation::RegexMatches(_, _) => {
            let pattern =
                Regex::new(right_value.as_str().expect("regex argument was not a string"))
                    .expect("regex argument was not a valid regex");
            apply_filter_op_with_static_argument(pattern, regex_matches_optimized, iterator)
        }
        Operation::NotRegexMatches(_, _) => {
            let pattern =
                Regex::new(right_value.as_str().expect("regex argument was not a string"))
                    .expect("regex argument was not a valid regex");
            apply_filter_op_with_static_argument(pattern, not!(regex_matches_optimized), iterator)
        }

        Operation::IsNull(_) | Operation::IsNotNull(_) => unreachable!("{filter:?}"),
    }
}

fn apply_filter_with_tagged_argument_value<'query, Vertex: Debug + Clone + 'query>(
    filter: &Operation<(), &Argument>,
    argument_value_iterator: ContextOutcomeIterator<'query, Vertex, TaggedValue>,
) -> ContextIterator<'query, Vertex> {
    match filter {
        Operation::Equals(_, _) => {
            apply_filter_op_with_tagged_argument(equals, argument_value_iterator)
        }
        Operation::NotEquals(_, _) => {
            apply_filter_op_with_tagged_argument(not!(equals), argument_value_iterator)
        }
        Operation::LessThan(_, _) => {
            apply_filter_op_with_tagged_argument(less_than, argument_value_iterator)
        }
        Operation::LessThanOrEqual(_, _) => {
            apply_filter_op_with_tagged_argument(less_than_or_equal, argument_value_iterator)
        }
        Operation::GreaterThan(_, _) => {
            apply_filter_op_with_tagged_argument(greater_than, argument_value_iterator)
        }
        Operation::GreaterThanOrEqual(_, _) => {
            apply_filter_op_with_tagged_argument(greater_than_or_equal, argument_value_iterator)
        }
        Operation::Contains(_, _) => {
            apply_filter_op_with_tagged_argument(contains, argument_value_iterator)
        }
        Operation::NotContains(_, _) => {
            apply_filter_op_with_tagged_argument(not!(contains), argument_value_iterator)
        }
        Operation::OneOf(_, _) => {
            apply_filter_op_with_tagged_argument(one_of, argument_value_iterator)
        }
        Operation::NotOneOf(_, _) => {
            apply_filter_op_with_tagged_argument(not!(one_of), argument_value_iterator)
        }
        Operation::HasPrefix(_, _) => {
            apply_filter_op_with_tagged_argument(has_prefix, argument_value_iterator)
        }
        Operation::NotHasPrefix(_, _) => {
            apply_filter_op_with_tagged_argument(not!(has_prefix), argument_value_iterator)
        }
        Operation::HasSuffix(_, _) => {
            apply_filter_op_with_tagged_argument(has_suffix, argument_value_iterator)
        }
        Operation::NotHasSuffix(_, _) => {
            apply_filter_op_with_tagged_argument(not!(has_suffix), argument_value_iterator)
        }
        Operation::HasSubstring(_, _) => {
            apply_filter_op_with_tagged_argument(has_substring, argument_value_iterator)
        }
        Operation::NotHasSubstring(_, _) => {
            apply_filter_op_with_tagged_argument(not!(has_substring), argument_value_iterator)
        }
        Operation::RegexMatches(_, _) => {
            apply_filter_op_with_tagged_argument(regex_matches_slow_path, argument_value_iterator)
        }
        Operation::NotRegexMatches(_, _) => apply_filter_op_with_tagged_argument(
            not!(regex_matches_slow_path),
            argument_value_iterator,
        ),
        Operation::IsNull(_) | Operation::IsNotNull(_) => unreachable!("{filter:?}"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        interpreter::filtering::{equals, greater_than_or_equal, less_than, less_than_or_equal},
        ir::FieldValue,
    };

    use super::greater_than;

    #[test]
    fn test_integer_strict_inequality_comparisons() {
        let test_data = [
            // both values can convert to each other
            (FieldValue::Uint64(0), FieldValue::Int64(0), false),
            (FieldValue::Uint64(0), FieldValue::Int64(1), false),
            (FieldValue::Uint64(1), FieldValue::Int64(0), true),
            //
            // the left value can convert into the right
            (FieldValue::Uint64(0), FieldValue::Int64(-1), true),
            //
            // the right value can convert into the left
            (FieldValue::Uint64(u64::MAX), FieldValue::Int64(2), true),
            //
            // neither value can convert into the other
            (FieldValue::Uint64(u64::MAX), FieldValue::Int64(-2), true),
        ];

        for (left, right, expected_outcome) in test_data {
            assert_eq!(expected_outcome, greater_than(&left, &right), "{left:?} > {right:?}",);
            assert_eq!(expected_outcome, less_than(&right, &left), "{right:?} < {left:?}",);
        }
    }

    #[test]
    fn test_integer_non_strict_inequality_comparisons() {
        let test_data = [
            // both values can convert to each other
            (FieldValue::Uint64(0), FieldValue::Int64(0), true),
            (FieldValue::Uint64(0), FieldValue::Int64(1), false),
            (FieldValue::Uint64(1), FieldValue::Int64(0), true),
            //
            // the left value can convert into the right
            (FieldValue::Uint64(0), FieldValue::Int64(-1), true),
            //
            // the right value can convert into the left
            (FieldValue::Uint64(u64::MAX), FieldValue::Int64(2), true),
            //
            // neither value can convert into the other
            (FieldValue::Uint64(u64::MAX), FieldValue::Int64(-2), true),
        ];

        for (left, right, expected_outcome) in test_data {
            assert_eq!(
                expected_outcome,
                greater_than_or_equal(&left, &right),
                "{left:?} >= {right:?}",
            );
            assert_eq!(
                expected_outcome,
                less_than_or_equal(&right, &left),
                "{right:?} <= {left:?}",
            );
        }
    }

    #[test]
    fn test_integer_equality_comparisons() {
        let test_data = [
            // both values can convert to each other
            (FieldValue::Uint64(0), FieldValue::Int64(0), true),
            (FieldValue::Uint64(0), FieldValue::Int64(1), false),
            (FieldValue::Uint64(1), FieldValue::Int64(0), false),
            //
            // the left value can convert into the right
            (FieldValue::Uint64(0), FieldValue::Int64(-1), false),
            //
            // the right value can convert into the left
            (FieldValue::Uint64(u64::MAX), FieldValue::Int64(2), false),
            //
            // neither value can convert into the other
            (FieldValue::Uint64(u64::MAX), FieldValue::Int64(-2), false),
        ];

        for (left, right, expected_outcome) in test_data {
            assert_eq!(expected_outcome, equals(&left, &right), "{left:?} = {right:?}",);
            assert_eq!(expected_outcome, equals(&right, &left), "{right:?} = {left:?}",);

            if expected_outcome {
                // both >= and <= comparisons in either direction should return true
                assert!(less_than_or_equal(&left, &right), "{left:?} <= {right:?}",);
                assert!(greater_than_or_equal(&left, &right), "{left:?} >= {right:?}",);
                assert!(less_than_or_equal(&right, &left), "{right:?} <= {left:?}",);
                assert!(greater_than_or_equal(&right, &left), "{right:?} >= {left:?}",);

                // both > and < comparisons in either direction should return false
                assert!(!less_than(&left, &right), "{left:?} < {right:?}");
                assert!(!greater_than(&left, &right), "{left:?} > {right:?}");
                assert!(!less_than(&right, &left), "{right:?} < {left:?}");
                assert!(!greater_than(&right, &left), "{right:?} > {left:?}");
            } else {
                // exactly one of <= / >= / < / > comparisons should return true per direction
                assert!(
                    less_than_or_equal(&left, &right) ^ greater_than_or_equal(&left, &right),
                    "{left:?} <= {right:?} ^ {left:?} >= {right:?}",
                );
                assert!(
                    less_than_or_equal(&right, &left) ^ greater_than_or_equal(&right, &left),
                    "{right:?} <= {left:?} ^ {right:?} >= {left:?}",
                );
                assert!(
                    less_than(&left, &right) ^ greater_than(&left, &right),
                    "{left:?} <= {right:?} ^ {left:?} >= {right:?}",
                );
                assert!(
                    less_than(&right, &left) ^ greater_than(&right, &left),
                    "{right:?} <= {left:?} ^ {right:?} >= {left:?}",
                );
            }
        }
    }

    #[test]
    fn test_mixed_list_equality_comparison() {
        let test_data = [
            (
                FieldValue::List(Arc::new([FieldValue::Uint64(0), FieldValue::Int64(0)])),
                FieldValue::List(Arc::new([FieldValue::Uint64(0), FieldValue::Int64(0)])),
                true,
            ),
            (
                FieldValue::List(Arc::new([FieldValue::Uint64(0), FieldValue::Int64(0)])),
                FieldValue::List(Arc::new([FieldValue::Int64(0), FieldValue::Uint64(0)])),
                true,
            ),
            (
                FieldValue::List(Arc::new([FieldValue::Int64(0), FieldValue::Uint64(0)])),
                FieldValue::List(Arc::new([FieldValue::Int64(0), FieldValue::Uint64(0)])),
                true,
            ),
            (
                FieldValue::List(Arc::new([FieldValue::Uint64(0), FieldValue::Int64(-2)])),
                FieldValue::List(Arc::new([FieldValue::Uint64(0), FieldValue::Int64(-2)])),
                true,
            ),
            (
                FieldValue::List(Arc::new([FieldValue::Int64(-1), FieldValue::Uint64(2)])),
                FieldValue::List(Arc::new([FieldValue::Int64(-1), FieldValue::Uint64(2)])),
                true,
            ),
            (
                FieldValue::List(Arc::new([FieldValue::Int64(-1), FieldValue::Uint64(2)])),
                FieldValue::List(Arc::new([FieldValue::Uint64(2), FieldValue::Int64(-1)])),
                false,
            ),
            (
                FieldValue::List(Arc::new([FieldValue::Uint64(0), FieldValue::Int64(0)])),
                FieldValue::List(Arc::new([FieldValue::Int64(0)])),
                false,
            ),
            (
                FieldValue::List(Arc::new([FieldValue::Uint64(0)])),
                FieldValue::List(Arc::new([])),
                false,
            ),
        ];

        for (left, right, expected_outcome) in test_data {
            assert_eq!(expected_outcome, equals(&left, &right), "{left:?} = {right:?}",);
            assert_eq!(expected_outcome, equals(&right, &left), "{right:?} = {left:?}",);
        }
    }
}
