use std::mem;

use regex::Regex;

use crate::ir::FieldValue;

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
                (FieldValue::DateTimeUtc(l), FieldValue::DateTimeUtc(r)) => l $op r,
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
        (FieldValue::String(l), FieldValue::String(r)) => l.contains(r),
        (FieldValue::Null, FieldValue::String(_))
        | (FieldValue::String(_), FieldValue::Null)
        | (FieldValue::Null, FieldValue::Null) => false,
        _ => unreachable!("{:?} {:?}", left, right),
    }
}

#[inline(always)]
pub(super) fn has_prefix(left: &FieldValue, right: &FieldValue) -> bool {
    match (left, right) {
        (FieldValue::String(l), FieldValue::String(r)) => l.starts_with(r),
        (FieldValue::Null, FieldValue::String(_))
        | (FieldValue::String(_), FieldValue::Null)
        | (FieldValue::Null, FieldValue::Null) => false,
        _ => unreachable!("{:?} {:?}", left, right),
    }
}

#[inline(always)]
pub(super) fn has_suffix(left: &FieldValue, right: &FieldValue) -> bool {
    match (left, right) {
        (FieldValue::String(l), FieldValue::String(r)) => l.ends_with(r),
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
            Regex::new(r)
                .map(|pattern| pattern.is_match(l))
                .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use crate::{
        interpreter::filtering::{equals, greater_than_or_equal, less_than, less_than_or_equal},
        ir::FieldValue,
    };

    use super::greater_than;

    #[test]
    fn test_integer_strict_inequality_comparisons() {
        let test_data = vec![
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
            assert_eq!(
                expected_outcome,
                greater_than(&left, &right),
                "{:?} > {:?}",
                left,
                right
            );
            assert_eq!(
                expected_outcome,
                less_than(&right, &left),
                "{:?} < {:?}",
                right,
                left
            );
        }
    }

    #[test]
    fn test_integer_non_strict_inequality_comparisons() {
        let test_data = vec![
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
                "{:?} >= {:?}",
                left,
                right
            );
            assert_eq!(
                expected_outcome,
                less_than_or_equal(&right, &left),
                "{:?} <= {:?}",
                right,
                left
            );
        }
    }

    #[test]
    fn test_integer_equality_comparisons() {
        let test_data = vec![
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
            assert_eq!(
                expected_outcome,
                equals(&left, &right),
                "{:?} = {:?}",
                left,
                right
            );
            assert_eq!(
                expected_outcome,
                equals(&right, &left),
                "{:?} = {:?}",
                right,
                left
            );

            if expected_outcome {
                // both >= and <= comparisons in either direction should return true
                assert!(
                    less_than_or_equal(&left, &right),
                    "{:?} <= {:?}",
                    left,
                    right
                );
                assert!(
                    greater_than_or_equal(&left, &right),
                    "{:?} >= {:?}",
                    left,
                    right
                );
                assert!(
                    less_than_or_equal(&right, &left),
                    "{:?} <= {:?}",
                    right,
                    left
                );
                assert!(
                    greater_than_or_equal(&right, &left),
                    "{:?} >= {:?}",
                    right,
                    left
                );

                // both > and < comparisons in either direction should return false
                assert!(!less_than(&left, &right), "{:?} < {:?}", left, right);
                assert!(!greater_than(&left, &right), "{:?} > {:?}", left, right);
                assert!(!less_than(&right, &left), "{:?} < {:?}", right, left);
                assert!(!greater_than(&right, &left), "{:?} > {:?}", right, left);
            } else {
                // exactly one of <= / >= / < / > comparisons should return true per direction
                assert!(
                    less_than_or_equal(&left, &right) ^ greater_than_or_equal(&left, &right),
                    "{:?} <= {:?} ^ {:?} >= {:?}",
                    left,
                    right,
                    left,
                    right
                );
                assert!(
                    less_than_or_equal(&right, &left) ^ greater_than_or_equal(&right, &left),
                    "{:?} <= {:?} ^ {:?} >= {:?}",
                    right,
                    left,
                    right,
                    left
                );
                assert!(
                    less_than(&left, &right) ^ greater_than(&left, &right),
                    "{:?} <= {:?} ^ {:?} >= {:?}",
                    left,
                    right,
                    left,
                    right
                );
                assert!(
                    less_than(&right, &left) ^ greater_than(&right, &left),
                    "{:?} <= {:?} ^ {:?} >= {:?}",
                    right,
                    left,
                    right,
                    left
                );
            }
        }
    }

    #[test]
    fn test_mixed_list_equality_comparison() {
        let test_data = vec![
            (
                FieldValue::List(vec![FieldValue::Uint64(0), FieldValue::Int64(0)]),
                FieldValue::List(vec![FieldValue::Uint64(0), FieldValue::Int64(0)]),
                true,
            ),
            (
                FieldValue::List(vec![FieldValue::Uint64(0), FieldValue::Int64(0)]),
                FieldValue::List(vec![FieldValue::Int64(0), FieldValue::Uint64(0)]),
                true,
            ),
            (
                FieldValue::List(vec![FieldValue::Int64(0), FieldValue::Uint64(0)]),
                FieldValue::List(vec![FieldValue::Int64(0), FieldValue::Uint64(0)]),
                true,
            ),
            (
                FieldValue::List(vec![FieldValue::Uint64(0), FieldValue::Int64(-2)]),
                FieldValue::List(vec![FieldValue::Uint64(0), FieldValue::Int64(-2)]),
                true,
            ),
            (
                FieldValue::List(vec![FieldValue::Int64(-1), FieldValue::Uint64(2)]),
                FieldValue::List(vec![FieldValue::Int64(-1), FieldValue::Uint64(2)]),
                true,
            ),
            (
                FieldValue::List(vec![FieldValue::Int64(-1), FieldValue::Uint64(2)]),
                FieldValue::List(vec![FieldValue::Uint64(2), FieldValue::Int64(-1)]),
                false,
            ),
            (
                FieldValue::List(vec![FieldValue::Uint64(0), FieldValue::Int64(0)]),
                FieldValue::List(vec![FieldValue::Int64(0)]),
                false,
            ),
            (
                FieldValue::List(vec![FieldValue::Uint64(0)]),
                FieldValue::List(vec![]),
                false,
            ),
        ];

        for (left, right, expected_outcome) in test_data {
            assert_eq!(
                expected_outcome,
                equals(&left, &right),
                "{:?} = {:?}",
                left,
                right
            );
            assert_eq!(
                expected_outcome,
                equals(&right, &left),
                "{:?} = {:?}",
                right,
                left
            );
        }
    }
}
