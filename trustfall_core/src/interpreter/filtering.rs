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

/// The comparison performed by a binary `@filter` operation, independent of where its
/// operands come from (a runtime argument, a `@tag` value, or a fold-specific field).
///
/// This is the single source of truth for filter comparison semantics: every filter path
/// in the engine (unary, runtime-argument, and tag-argument filters) dispatches through
/// [`ComparisonOp::apply`], so the sync and async execution paths cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ComparisonOp {
    Equals,
    NotEquals,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    Contains,
    NotContains,
    OneOf,
    NotOneOf,
    HasPrefix,
    NotHasPrefix,
    HasSuffix,
    NotHasSuffix,
    HasSubstring,
    NotHasSubstring,
    RegexMatches,
    NotRegexMatches,
}

impl ComparisonOp {
    /// The comparison performed by a binary filter operation, or `None` for unary operations.
    pub(super) fn from_binary_filter(filter: &Operation<(), &Argument>) -> Option<Self> {
        match filter {
            Operation::Equals(..) => Some(Self::Equals),
            Operation::NotEquals(..) => Some(Self::NotEquals),
            Operation::LessThan(..) => Some(Self::LessThan),
            Operation::LessThanOrEqual(..) => Some(Self::LessThanOrEqual),
            Operation::GreaterThan(..) => Some(Self::GreaterThan),
            Operation::GreaterThanOrEqual(..) => Some(Self::GreaterThanOrEqual),
            Operation::Contains(..) => Some(Self::Contains),
            Operation::NotContains(..) => Some(Self::NotContains),
            Operation::OneOf(..) => Some(Self::OneOf),
            Operation::NotOneOf(..) => Some(Self::NotOneOf),
            Operation::HasPrefix(..) => Some(Self::HasPrefix),
            Operation::NotHasPrefix(..) => Some(Self::NotHasPrefix),
            Operation::HasSuffix(..) => Some(Self::HasSuffix),
            Operation::NotHasSuffix(..) => Some(Self::NotHasSuffix),
            Operation::HasSubstring(..) => Some(Self::HasSubstring),
            Operation::NotHasSubstring(..) => Some(Self::NotHasSubstring),
            Operation::RegexMatches(..) => Some(Self::RegexMatches),
            Operation::NotRegexMatches(..) => Some(Self::NotRegexMatches),
            Operation::IsNull(..) | Operation::IsNotNull(..) => None,
        }
    }

    /// Apply this operation to the filter's left-hand (field) and right-hand values.
    ///
    /// Regex operations compile their pattern on every call; when the right-hand value is
    /// known ahead of time, prefer [`ValuePredicate::static_argument`] which precompiles it.
    #[inline]
    pub(super) fn apply(self, left: &FieldValue, right: &FieldValue) -> bool {
        match self {
            Self::Equals => equals(left, right),
            Self::NotEquals => !equals(left, right),
            Self::LessThan => less_than(left, right),
            Self::LessThanOrEqual => less_than_or_equal(left, right),
            Self::GreaterThan => greater_than(left, right),
            Self::GreaterThanOrEqual => greater_than_or_equal(left, right),
            Self::Contains => contains(left, right),
            Self::NotContains => !contains(left, right),
            Self::OneOf => one_of(left, right),
            Self::NotOneOf => !one_of(left, right),
            Self::HasPrefix => has_prefix(left, right),
            Self::NotHasPrefix => !has_prefix(left, right),
            Self::HasSuffix => has_suffix(left, right),
            Self::NotHasSuffix => !has_suffix(left, right),
            Self::HasSubstring => has_substring(left, right),
            Self::NotHasSubstring => !has_substring(left, right),
            Self::RegexMatches => regex_matches_slow_path(left, right),
            Self::NotRegexMatches => !regex_matches_slow_path(left, right),
        }
    }
}

/// The per-value predicate of a `@filter` whose right-hand side is known without an
/// adapter call: a unary operation, or a binary operation against a runtime argument.
///
/// Stored by value in the pipeline's filter stage (no boxed closures): applying it is
/// a direct call, and regexes against runtime arguments are compiled exactly once.
#[derive(Debug)]
pub(super) enum ValuePredicate {
    /// `@filter(op: "is_null")`
    IsNull,
    /// `@filter(op: "is_not_null")`
    IsNotNull,
    /// A binary operation against a value known when the pipeline is constructed.
    Static { op: ComparisonOp, right: FieldValue },
    /// A regex operation whose pattern is known when the pipeline is constructed.
    StaticRegex { negated: bool, pattern: Regex },
}

impl ValuePredicate {
    /// The predicate of a unary filter (`is_null` / `is_not_null`),
    /// or `None` if `filter` is a binary operation.
    pub(super) fn unary(filter: &Operation<(), &Argument>) -> Option<Self> {
        match filter {
            Operation::IsNull(_) => Some(Self::IsNull),
            Operation::IsNotNull(_) => Some(Self::IsNotNull),
            _ => None,
        }
    }

    /// The predicate of a binary filter whose right-hand side is the given runtime
    /// argument value. Panics for unary filters (use [`ValuePredicate::unary`]) and is
    /// not applicable to tag arguments, whose values are resolved per context.
    pub(super) fn static_argument(
        filter: &Operation<(), &Argument>,
        right_value: FieldValue,
    ) -> Self {
        match ComparisonOp::from_binary_filter(filter) {
            Some(ComparisonOp::RegexMatches | ComparisonOp::NotRegexMatches) => {
                let negated = matches!(filter, Operation::NotRegexMatches(..));
                let pattern =
                    Regex::new(right_value.as_str().expect("regex argument was not a string"))
                        .expect("regex argument was not a valid regex");
                Self::StaticRegex { negated, pattern }
            }
            Some(op) => Self::Static { op, right: right_value },
            None => unreachable!("unary filter passed to ValuePredicate::static_argument: {filter:?}"),
        }
    }

    /// Whether the filter's left-hand value passes this predicate.
    #[inline]
    pub(super) fn passes(&self, left: &FieldValue) -> bool {
        match self {
            Self::IsNull => is_null(left),
            Self::IsNotNull => !is_null(left),
            Self::Static { op, right } => op.apply(left, right),
            Self::StaticRegex { negated, pattern } => {
                regex_matches_optimized(left, pattern) ^ *negated
            }
        }
    }
}
