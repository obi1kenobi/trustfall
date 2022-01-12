use regex::Regex;

use crate::ir::FieldValue;

#[inline(always)]
pub(super) fn equals(left: &FieldValue, right: &FieldValue) -> bool {
    left == right
}

macro_rules! make_comparison_op_func {
    ( $func: ident, $op: tt ) => {
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
                _ => unreachable!(),
            }
        }
    };
}

make_comparison_op_func!(greater_than, >);
make_comparison_op_func!(greater_than_or_equal, >=);
make_comparison_op_func!(less_than, <);
make_comparison_op_func!(less_than_or_equal, <=);

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
