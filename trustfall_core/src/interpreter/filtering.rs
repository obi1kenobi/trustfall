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
            None => {
                unreachable!("unary filter passed to ValuePredicate::static_argument: {filter:?}")
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Eid, FieldRef, FoldSpecificField, FoldSpecificFieldKind, Vid};
    use std::num::NonZeroUsize;

    fn s(value: &str) -> FieldValue {
        FieldValue::String(std::sync::Arc::from(value))
    }

    fn list(values: &[FieldValue]) -> FieldValue {
        FieldValue::List(std::sync::Arc::from(values))
    }

    /// Scalar value pairs that every binary comparison must handle.
    ///
    /// Note: comparisons between mismatched non-integer kinds (e.g. string vs. int,
    /// or lists) are contract violations on the adapter's part and panic in the slow
    /// paths, so they deliberately do not appear here.
    fn scalar_pairs() -> Vec<(&'static str, FieldValue, FieldValue)> {
        vec![
            ("int/int", FieldValue::Int64(2), FieldValue::Int64(3)),
            ("int/int eq", FieldValue::Int64(3), FieldValue::Int64(3)),
            ("uint/uint", FieldValue::Uint64(2), FieldValue::Uint64(3)),
            ("int/uint cross", FieldValue::Int64(3), FieldValue::Uint64(3)),
            ("uint/int cross", FieldValue::Uint64(3), FieldValue::Int64(3)),
            ("int neg/uint", FieldValue::Int64(-1), FieldValue::Uint64(1)),
            ("float/float", FieldValue::Float64(2.5), FieldValue::Float64(2.75)),
            ("float/float eq", FieldValue::Float64(2.5), FieldValue::Float64(2.5)),
            ("string/string", s("hello"), s("hel")),
            ("string/string eq", s("hello"), s("hello")),
            ("null left", FieldValue::Null, FieldValue::Int64(0)),
            ("null right", s("x"), FieldValue::Null),
            ("null/null", FieldValue::Null, FieldValue::Null),
        ]
    }

    /// Pairs where only textual operations apply (strings and nulls); ordering
    /// comparisons on strings are valid too, so they appear in `scalar_pairs`.
    fn textual_pairs() -> Vec<(&'static str, FieldValue, FieldValue)> {
        vec![
            ("string/string", s("hello"), s("hel")),
            ("string/string eq", s("hello"), s("hello")),
            ("string/null", s("x"), FieldValue::Null),
            ("null/string", FieldValue::Null, s("x")),
            ("null/null", FieldValue::Null, FieldValue::Null),
        ]
    }

    #[test]
    fn comparison_op_golden_table() {
        let scalar = scalar_pairs();
        // (op, expected over `scalar_pairs` order)
        let ordering: Vec<(ComparisonOp, Vec<bool>)> = vec![
            (
                ComparisonOp::Equals,
                vec![
                    false, true, false, true, true, false, false, true, false, true, false, false,
                    true,
                ],
            ),
            (
                ComparisonOp::NotEquals,
                vec![
                    true, false, true, false, false, true, true, false, true, false, true, true,
                    false,
                ],
            ),
            (
                ComparisonOp::LessThan,
                vec![
                    true, false, true, false, false, true, true, false, false, false, false, false,
                    false,
                ],
            ),
            (
                ComparisonOp::LessThanOrEqual,
                vec![
                    true, true, true, true, true, true, true, true, false, true, false, false,
                    false,
                ],
            ),
            (
                ComparisonOp::GreaterThan,
                vec![
                    false, false, false, false, false, false, false, false, true, false, false,
                    false, false,
                ],
            ),
            (
                ComparisonOp::GreaterThanOrEqual,
                vec![
                    false, true, false, true, true, false, false, true, true, true, false, false,
                    false,
                ],
            ),
        ];
        for (op, expected) in ordering {
            for ((label, left, right), want) in scalar.iter().zip(expected) {
                assert_eq!(
                    op.apply(left, right),
                    want,
                    "{op:?}({label}): left {left:?} right {right:?}",
                );
            }
        }

        let textual = textual_pairs();
        let textual_ops: Vec<(ComparisonOp, Vec<bool>)> = vec![
            (ComparisonOp::HasPrefix, vec![true, true, false, false, false]),
            (ComparisonOp::NotHasPrefix, vec![false, false, true, true, true]),
            (ComparisonOp::HasSuffix, vec![false, true, false, false, false]),
            (ComparisonOp::NotHasSuffix, vec![true, false, true, true, true]),
            (ComparisonOp::HasSubstring, vec![true, true, false, false, false]),
            (ComparisonOp::NotHasSubstring, vec![false, false, true, true, true]),
            (ComparisonOp::RegexMatches, vec![true, true, false, false, false]),
            (ComparisonOp::NotRegexMatches, vec![false, false, true, true, true]),
        ];
        for (op, expected) in textual_ops {
            for ((label, left, right), want) in textual.iter().zip(expected) {
                assert_eq!(
                    op.apply(left, right),
                    want,
                    "{op:?}({label}): left {left:?} right {right:?}",
                );
            }
        }
    }

    /// Values that only support (in)equality: lists, and integer pairs outside the other
    /// type's range where ordering comparisons are documented to panic instead.
    #[test]
    fn comparison_op_equality_only_values() {
        // List equality recurses element-wise.
        let l1 = list(&[FieldValue::Int64(1), FieldValue::Int64(2)]);
        let l2 = list(&[FieldValue::Int64(1), FieldValue::Int64(2)]);
        let l3 = list(&[FieldValue::Int64(1)]);
        assert!(ComparisonOp::Equals.apply(&l1, &l2));
        assert!(ComparisonOp::NotEquals.apply(&l1, &l3));
        assert!(!ComparisonOp::Equals.apply(&l1, &l3));

        // u64::MAX vs -1: representable in neither other type; equality is false, not a panic.
        let big = FieldValue::Uint64(u64::MAX);
        let neg = FieldValue::Int64(-1);
        assert!(!ComparisonOp::Equals.apply(&big, &neg));
        assert!(!ComparisonOp::Equals.apply(&neg, &big));
        assert!(ComparisonOp::NotEquals.apply(&big, &neg));

        // one_of: right must be a list; contains is one_of with operands swapped.
        let left = FieldValue::Int64(2);
        let members = list(&[FieldValue::Int64(1), FieldValue::Int64(2)]);
        assert!(ComparisonOp::OneOf.apply(&left, &members));
        assert!(!ComparisonOp::NotOneOf.apply(&left, &members));
        // A field holding a list "contains" a member value.
        assert!(ComparisonOp::Contains.apply(&members, &left));
        // one_of against a null list is false, not a panic.
        assert!(!ComparisonOp::OneOf.apply(&left, &FieldValue::Null));

        // Regexes: an invalid pattern matches nothing.
        assert!(!ComparisonOp::RegexMatches.apply(&s("anything"), &s("(unclosed")));
        assert!(ComparisonOp::NotRegexMatches.apply(&s("anything"), &s("(unclosed")));
    }

    #[test]
    fn value_predicate_matches_comparison_op() {
        let ordering_ops = [
            ComparisonOp::Equals,
            ComparisonOp::NotEquals,
            ComparisonOp::LessThan,
            ComparisonOp::LessThanOrEqual,
            ComparisonOp::GreaterThan,
            ComparisonOp::GreaterThanOrEqual,
        ];
        for op in ordering_ops {
            for (_, left, right) in scalar_pairs() {
                let predicate = ValuePredicate::Static { op, right: right.clone() };
                assert_eq!(
                    predicate.passes(&left),
                    op.apply(&left, &right),
                    "ValuePredicate({op:?}) diverged from ComparisonOp::apply",
                );
            }
        }
        for op in [ComparisonOp::HasPrefix, ComparisonOp::NotHasSubstring] {
            for (_, left, right) in textual_pairs() {
                let predicate = ValuePredicate::Static { op, right: right.clone() };
                assert_eq!(predicate.passes(&left), op.apply(&left, &right), "{op:?}");
            }
        }

        // Precompiled regex predicates agree with the slow path.
        let pattern = Regex::new("^ab").expect("valid regex");
        let slow_path_pattern = s("^ab");
        for candidate in ["ab", "ba", "abc"] {
            let value = s(candidate);
            let predicate =
                ValuePredicate::StaticRegex { negated: false, pattern: pattern.clone() };
            assert_eq!(
                predicate.passes(&value),
                ComparisonOp::RegexMatches.apply(&value, &slow_path_pattern),
                "precompiled regex diverged for {candidate:?}",
            );
            let negated = ValuePredicate::StaticRegex { negated: true, pattern: pattern.clone() };
            assert_eq!(negated.passes(&value), !predicate.passes(&value));
        }
    }

    #[test]
    fn unary_predicates() {
        let is_null = ValuePredicate::unary(&Operation::IsNull(())).unwrap();
        assert!(is_null.passes(&FieldValue::Null));
        assert!(!is_null.passes(&FieldValue::Int64(0)));

        let is_not_null =
            ValuePredicate::unary(&Operation::<(), &Argument>::IsNotNull(())).unwrap();
        assert!(!is_not_null.passes(&FieldValue::Null));
        assert!(is_not_null.passes(&FieldValue::Int64(0)));

        assert!(
            ValuePredicate::unary(&Operation::<(), &Argument>::Equals((), null_arg())).is_none()
        );
    }

    fn null_arg() -> &'static Argument {
        // Only the operation kind matters to `ValuePredicate::unary`, never the argument.
        static NONE: std::sync::OnceLock<Argument> = std::sync::OnceLock::new();
        NONE.get_or_init(|| {
            Argument::Tag(FieldRef::FoldSpecificField(FoldSpecificField {
                fold_eid: Eid::new(NonZeroUsize::new(1).unwrap()),
                fold_root_vid: Vid::new(NonZeroUsize::new(1).unwrap()),
                kind: FoldSpecificFieldKind::Count,
            }))
        })
    }
}
