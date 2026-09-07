use std::mem;

use regex::Regex;

use crate::ir::{Argument, FieldValue, Operation};

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

#[inline(always)]
fn is_null(value: &FieldValue) -> bool {
    matches!(value, FieldValue::Null)
}

/// The per-value comparison of a unary filter (`@filter(op: "is_null")` / `"is_not_null"`),
/// or `None` if `filter` is not a unary operation.
///
/// Shared with the async engine so both engines apply identical filter semantics.
pub(super) fn unary_filter_predicate(
    filter: &Operation<(), &Argument>,
) -> Option<fn(&FieldValue) -> bool> {
    match filter {
        Operation::IsNull(_) => Some(is_null),
        Operation::IsNotNull(_) => Some(|value| !is_null(value)),
        _ => None,
    }
}

/// The per-value comparison of a filter whose right-hand side is a static (runtime-argument) value,
/// e.g. `equals` bound to the provided `right_value`. Panics on unary ops (use
/// [`unary_filter_predicate`]) and is not applicable to tag arguments.
///
/// Shared with the async engine so both engines apply identical filter semantics.
pub(super) fn static_argument_filter_predicate<'query>(
    filter: &Operation<(), &Argument>,
    right_value: FieldValue,
) -> Box<dyn Fn(&FieldValue) -> bool + 'query> {
    macro_rules! bind {
        ($op:expr) => {
            Box::new(move |left: &FieldValue| $op(left, &right_value))
        };
    }
    match filter {
        Operation::Equals(_, _) => bind!(equals),
        Operation::NotEquals(_, _) => bind!(|l, r| !equals(l, r)),
        Operation::LessThan(_, _) => bind!(less_than),
        Operation::LessThanOrEqual(_, _) => bind!(less_than_or_equal),
        Operation::GreaterThan(_, _) => bind!(greater_than),
        Operation::GreaterThanOrEqual(_, _) => bind!(greater_than_or_equal),
        Operation::Contains(_, _) => bind!(contains),
        Operation::NotContains(_, _) => bind!(|l, r| !contains(l, r)),
        Operation::OneOf(_, _) => bind!(one_of),
        Operation::NotOneOf(_, _) => bind!(|l, r| !one_of(l, r)),
        Operation::HasPrefix(_, _) => bind!(has_prefix),
        Operation::NotHasPrefix(_, _) => bind!(|l, r| !has_prefix(l, r)),
        Operation::HasSuffix(_, _) => bind!(has_suffix),
        Operation::NotHasSuffix(_, _) => bind!(|l, r| !has_suffix(l, r)),
        Operation::HasSubstring(_, _) => bind!(has_substring),
        Operation::NotHasSubstring(_, _) => bind!(|l, r| !has_substring(l, r)),
        Operation::RegexMatches(_, _) => {
            let pattern =
                Regex::new(right_value.as_str().expect("regex argument was not a string"))
                    .expect("regex argument was not a valid regex");
            Box::new(move |left: &FieldValue| regex_matches_optimized(left, &pattern))
        }
        Operation::NotRegexMatches(_, _) => {
            let pattern =
                Regex::new(right_value.as_str().expect("regex argument was not a string"))
                    .expect("regex argument was not a valid regex");
            Box::new(move |left: &FieldValue| !regex_matches_optimized(left, &pattern))
        }
        Operation::IsNull(_) | Operation::IsNotNull(_) => {
            unreachable!("unary filter passed to static_argument_filter_predicate: {filter:?}")
        }
    }
}
