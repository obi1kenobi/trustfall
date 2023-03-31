#![allow(dead_code)]

use std::{fmt::Debug, ops::Bound};

use serde::{Deserialize, Serialize};

use crate::ir::FieldValue;

/// Candidate values for the value of a vertex property.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateValue<T> {
    /// This property has no valid value that satisfies the query.
    Impossible,

    /// There's only one value that could satisfy the query.
    Single(T),

    /// There are multiple discrete values that could satisfy the query.
    Multiple(Vec<T>),

    /// A continuous range of values for this property could satisfy this query.
    Range(Range<T>),

    /// We've detected no constraints on the value of this property.
    All,
}

impl<T: Debug + Clone + PartialEq + Eq + PartialOrd + NullableValue + Default> CandidateValue<T> {
    pub(super) fn intersect(&mut self, mut other: CandidateValue<T>) {
        match self {
            Self::Impossible => {} // still impossible
            Self::Single(val) => {
                // It can only be single value,
                // but might become impossible depending on the other side.
                match other {
                    Self::Impossible => *self = CandidateValue::Impossible,
                    Self::Single(other) => {
                        if val != &other {
                            *self = CandidateValue::Impossible;
                        }
                    }
                    Self::Multiple(others) => {
                        if !others.contains(val) {
                            *self = CandidateValue::Impossible;
                        }
                    }
                    Self::Range(others) => {
                        if !others.contains(val) {
                            *self = CandidateValue::Impossible;
                        }
                    }
                    Self::All => {} // self is unchanged.
                }
            }
            Self::Multiple(multiple) => {
                match other {
                    Self::Impossible => *self = CandidateValue::Impossible,
                    Self::Single(other) => {
                        // The other side can only be a single value.
                        // The result is either only a single value or impossible
                        // depending on whether there's overlap.
                        if multiple.contains(&other) {
                            *self = Self::Single(other);
                        } else {
                            *self = Self::Impossible;
                        }
                    }
                    Self::Multiple(_) | Self::Range(_) => {
                        // We normalize at the end, for now let's just
                        // eliminate the disallowed values.
                        if let Self::Multiple(others) = &other {
                            multiple.retain(|value| others.contains(value));
                        } else if let Self::Range(others) = &other {
                            multiple.retain(|value| others.contains(value));
                        } else {
                            unreachable!("expected only Multiple or Range in this branch, but got: {other:?}");
                        }
                    }
                    Self::All => {} // self is unchanged.
                }
            }
            Self::Range(range) => {
                if let CandidateValue::Range(other) = other {
                    range.intersect(other)
                } else {
                    // We've already handled this case, just with operands reversed.
                    let mut placeholder = CandidateValue::All;
                    std::mem::swap(self, &mut placeholder);
                    other.intersect(placeholder);
                    *self = other;
                }
            }
            Self::All => {
                // Whatever the other candidate was. It can't be any wider than Self::All.
                *self = other;
            }
        }

        self.normalize();
    }

    pub(super) fn exclude_single_value(&mut self, value: &T) {
        match self {
            CandidateValue::Impossible => {} // nothing further to exclude
            CandidateValue::Single(s) => {
                if &*s == value {
                    *self = CandidateValue::Impossible;
                }
            }
            CandidateValue::Multiple(multiple) => {
                multiple.retain(|v| v != value);
                self.normalize();
            }
            CandidateValue::Range(range) => {
                if value.is_null() {
                    range.null_included = false;
                } else {
                    // TODO: When the values are integers, we can do better here:
                    //       we can move from one included bound to another, tighter included bound.
                    //       This can allow subsequent value exclusions through other heuristics.
                    if let Bound::Included(incl) = range.start_bound() {
                        if incl == value {
                            range.start = Bound::Excluded(incl.clone());
                        }
                    }
                    if let Bound::Included(incl) = range.end_bound() {
                        if incl == value {
                            range.end = Bound::Excluded(incl.clone());
                        }
                    }
                }
                self.normalize();
            }
            CandidateValue::All => {
                // We can only meaningfully exclude null values from the full range.
                //
                // TODO: In principle, we can also exclude the extreme values of integers,
                //       if we want to special-case this to the type of values.
                if value.is_null() {
                    *self = CandidateValue::Range(Range::full_non_null())
                }
            }
        }
    }

    pub(super) fn normalize(&mut self) {
        let next_self = if let Self::Range(range) = self {
            if range.null_only() {
                Some(Self::Single(T::default()))
            } else if range.degenerate() {
                Some(CandidateValue::Impossible)
            } else if range.start_bound() == range.end_bound() {
                if *range == Range::full() {
                    Some(CandidateValue::All)
                } else if let Bound::Included(b) = range.start_bound() {
                    // If the range is point-like (possibly +null), convert it to discrete values.
                    if range.null_included() {
                        Some(Self::Multiple(vec![T::default(), b.clone()]))
                    } else {
                        Some(Self::Single(b.clone()))
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else if let Self::Multiple(values) = self {
            if values.is_empty() {
                Some(Self::Impossible)
            } else if values.len() == 1 {
                Some(Self::Single(values.pop().expect("no value present")))
            } else {
                None
            }
        } else {
            None
        };

        if let Some(next_self) = next_self {
            *self = next_self;
        }
    }
}

pub trait NullableValue {
    fn is_null(&self) -> bool;
}

impl NullableValue for FieldValue {
    fn is_null(&self) -> bool {
        matches!(self, FieldValue::Null)
    }
}

impl NullableValue for &FieldValue {
    fn is_null(&self) -> bool {
        matches!(*self, FieldValue::Null)
    }
}

/// A range of values. Both its endpoints may be included or excluded in the range, or unbounded.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range<T> {
    start: Bound<T>,
    end: Bound<T>,
    null_included: bool,
}

impl<T: Debug + Clone + PartialEq + Eq + PartialOrd + NullableValue> Range<T> {
    /// The full, unbounded range of values.
    pub const fn full() -> Range<T> {
        Self {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
            null_included: true,
        }
    }

    /// The full range of values, except null.
    pub const fn full_non_null() -> Range<T> {
        Self {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
            null_included: false,
        }
    }

    pub(super) fn new(start: Bound<T>, end: Bound<T>, null_included: bool) -> Self {
        match &start {
            Bound::Included(v) | Bound::Excluded(v) => {
                assert!(!v.is_null(), "cannot bound range with null value")
            }
            Bound::Unbounded => {}
        }
        match &end {
            Bound::Included(v) | Bound::Excluded(v) => {
                assert!(!v.is_null(), "cannot bound range with null value")
            }
            Bound::Unbounded => {}
        }
        Self {
            start,
            end,
            null_included,
        }
    }

    pub(super) fn with_start(start: Bound<T>, null_included: bool) -> Self {
        match &start {
            Bound::Included(v) | Bound::Excluded(v) => {
                assert!(!v.is_null(), "cannot bound range with null value")
            }
            Bound::Unbounded => {}
        }
        Self {
            start,
            end: Bound::Unbounded,
            null_included,
        }
    }

    pub(super) fn with_end(end: Bound<T>, null_included: bool) -> Self {
        match &end {
            Bound::Included(v) | Bound::Excluded(v) => {
                assert!(!v.is_null(), "cannot bound range with null value")
            }
            Bound::Unbounded => {}
        }
        Self {
            start: Bound::Unbounded,
            end,
            null_included,
        }
    }

    pub(super) fn intersect(&mut self, other: Range<T>) {
        match &mut self.start {
            Bound::Included(start) => {
                debug_assert!(!start.is_null());
                match &other.start {
                    Bound::Included(other_start) => {
                        debug_assert!(!other_start.is_null());
                        if &*start < other_start {
                            self.start = other.start;
                        }
                    }
                    Bound::Excluded(other_start) => {
                        debug_assert!(!other_start.is_null());
                        if &*start <= other_start {
                            // self.end should become a Bound::Excluded
                            self.start = other.start;
                        }
                    }
                    Bound::Unbounded => {}
                }
            }
            Bound::Excluded(start) => {
                debug_assert!(!start.is_null());
                match &other.start {
                    Bound::Included(other_start) | Bound::Excluded(other_start) => {
                        debug_assert!(!other_start.is_null());
                        if &*start < other_start {
                            self.start = other.start;
                        }
                    }
                    Bound::Unbounded => {}
                }
            }
            Bound::Unbounded => self.start = other.start,
        }

        match &mut self.end {
            Bound::Included(end) => {
                debug_assert!(!end.is_null());
                match &other.end {
                    Bound::Included(other_end) => {
                        debug_assert!(!other_end.is_null());
                        if &*end > other_end {
                            self.end = other.end;
                        }
                    }
                    Bound::Excluded(other_end) => {
                        debug_assert!(!other_end.is_null());
                        if &*end >= other_end {
                            // self.end should become a Bound::Excluded
                            self.end = other.end;
                        }
                    }
                    Bound::Unbounded => {}
                }
            }
            Bound::Excluded(end) => {
                debug_assert!(!end.is_null());
                match &other.end {
                    Bound::Included(other_end) | Bound::Excluded(other_end) => {
                        debug_assert!(!other_end.is_null());
                        if &*end > other_end {
                            self.end = other.end;
                        }
                    }
                    Bound::Unbounded => {}
                }
            }
            Bound::Unbounded => self.end = other.end,
        }

        self.null_included &= other.null_included;
    }

    #[inline]
    pub fn start_bound(&self) -> Bound<&T> {
        self.start.as_ref()
    }

    #[inline]
    pub fn end_bound(&self) -> Bound<&T> {
        self.end.as_ref()
    }

    #[inline]
    pub fn null_included(&self) -> bool {
        self.null_included
    }

    #[inline]
    pub fn degenerate(&self) -> bool {
        match (self.start_bound(), self.end_bound()) {
            (Bound::Included(l), Bound::Included(r)) => l > r,
            (Bound::Included(l), Bound::Excluded(r))
            | (Bound::Excluded(l), Bound::Included(r))
            | (Bound::Excluded(l), Bound::Excluded(r)) => l >= r,
            (_, Bound::Unbounded) | (Bound::Unbounded, _) => false,
        }
    }

    #[inline]
    pub fn null_only(&self) -> bool {
        self.null_included && self.degenerate()
    }

    pub fn contains(&self, item: &T) -> bool {
        let is_null = item.is_null();
        if is_null {
            self.null_included
        } else {
            (match self.start_bound() {
                Bound::Included(start) => start <= item,
                Bound::Excluded(start) => start < item,
                Bound::Unbounded => true,
            }) && (match self.end_bound() {
                Bound::Included(end) => item <= end,
                Bound::Excluded(end) => item < end,
                Bound::Unbounded => true,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Bound;

    use itertools::Itertools;

    use crate::ir::FieldValue;

    use super::CandidateValue;

    #[test]
    fn candidate_intersecting() {
        use super::Range as R;
        use CandidateValue::*;
        let one = FieldValue::Int64(1);
        let two = FieldValue::Int64(2);
        let three = FieldValue::Int64(3);
        let four = FieldValue::Int64(4);

        let test_cases = [
            // Anything merged into Impossible is Impossible.
            (Impossible, Impossible, Impossible),
            (Impossible, Single(&one), Impossible),
            (Impossible, Multiple(vec![&one, &two]), Impossible),
            (
                Impossible,
                Range(R::with_start(Bound::Included(&one), true)),
                Impossible,
            ),
            //
            // Intersecting Impossible into anything produces Imposssible.
            (Single(&one), Impossible, Impossible),
            (Multiple(vec![&one, &two]), Impossible, Impossible),
            (
                Range(R::with_start(Bound::Included(&one), true)),
                Impossible,
                Impossible,
            ),
            //
            // Intersecting null into non-null, or vice versa, produces Impossible.
            (Single(&FieldValue::NULL), Single(&one), Impossible),
            (
                Single(&FieldValue::NULL),
                Multiple(vec![&one, &two]),
                Impossible,
            ),
            (
                Single(&FieldValue::NULL),
                Range(R::with_start(Bound::Included(&one), false)),
                Impossible,
            ),
            //
            // Intersecting non-overlapping single or multiple values produces Impossible.
            (Single(&one), Single(&two), Impossible),
            (Single(&one), Multiple(vec![&two, &three]), Impossible),
            (Multiple(vec![&one, &two]), Single(&three), Impossible),
            (
                Multiple(vec![&one, &two]),
                Multiple(vec![&three, &four]),
                Impossible,
            ),
            //
            // Intersecting values with a non-overlapping range produces Impossible.
            (
                Single(&one),
                Range(R::with_start(Bound::Excluded(&one), true)),
                Impossible,
            ),
            (
                Single(&one),
                Range(R::with_start(Bound::Included(&two), false)),
                Impossible,
            ),
            (
                Multiple(vec![&two, &three]),
                Range(R::with_end(Bound::Included(&one), true)),
                Impossible,
            ),
            (
                Multiple(vec![&two, &three]),
                Range(R::with_end(Bound::Excluded(&two), false)),
                Impossible,
            ),
            //
            // Intersecting overlapping single values, or single with multiple or a range,
            // produces the overlapping Single.
            (Single(&one), Single(&one), Single(&one)),
            (Multiple(vec![&one, &two]), Single(&one), Single(&one)),
            (Single(&one), Multiple(vec![&one, &two]), Single(&one)),
            (
                Single(&one),
                Range(R::with_start(Bound::Included(&one), false)),
                Single(&one),
            ),
            (
                Single(&one),
                Range(R::with_end(Bound::Excluded(&two), false)),
                Single(&one),
            ),
            //
            // Intersecting null into multiple or a range that contains null produces null.
            (
                Single(&FieldValue::NULL),
                Multiple(vec![&one, &FieldValue::Null]),
                Single(&FieldValue::NULL),
            ),
            (
                Single(&FieldValue::NULL),
                Range(R::with_end(Bound::Excluded(&two), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Single(&FieldValue::NULL),
                Range(R::with_start(Bound::Excluded(&two), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Single(&FieldValue::NULL),
                Range(R::new(Bound::Unbounded, Bound::Unbounded, true)),
                Single(&FieldValue::NULL),
            ),
            (
                Single(&FieldValue::NULL),
                Range(R::new(Bound::Excluded(&one), Bound::Excluded(&one), true)),
                Single(&FieldValue::NULL),
            ),
            //
            // Intersecting multiple values that include null works correctly too.
            (
                Multiple(vec![&one, &FieldValue::Null, &two, &three]),
                Multiple(vec![&one, &FieldValue::Null, &four]),
                Multiple(vec![&one, &FieldValue::Null]),
            ),
            //
            // Intersecting ranges that only overlap on null produces Single(null).
            (
                Range(R::new(Bound::Included(&one), Bound::Included(&one), true)),
                Range(R::new(Bound::Included(&two), Bound::Included(&two), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Range(R::new(Bound::Included(&one), Bound::Excluded(&two), true)),
                Range(R::new(Bound::Included(&two), Bound::Excluded(&three), true)),
                Single(&FieldValue::NULL),
            ),
            //
            // Intersecting ranges that only overlap on a single non-null value produces Single.
            (
                Range(R::new(Bound::Included(&one), Bound::Included(&two), false)),
                Range(R::new(Bound::Included(&two), Bound::Included(&three), true)),
                Single(&two),
            ),
            //
            // Intersecting ranges that don't overlap at all produces Impossible.
            (
                Range(R::new(Bound::Included(&one), Bound::Included(&one), true)),
                Range(R::new(Bound::Included(&two), Bound::Included(&two), false)),
                Impossible,
            ),
            (
                Range(R::new(Bound::Included(&one), Bound::Excluded(&two), false)),
                Range(R::new(Bound::Included(&two), Bound::Excluded(&three), true)),
                Impossible,
            ),
            //
            // Intersecting ranges that only overlap on null and exactly one other value
            // produces Multiple(null, that value).
            (
                Range(R::new(Bound::Included(&one), Bound::Included(&two), true)),
                Range(R::new(Bound::Included(&two), Bound::Included(&three), true)),
                Multiple(vec![&FieldValue::NULL, &two]),
            ),
            //
            // Intersecting ranges that overlap on a range produces the overlapping range.
            (
                Range(R::new(Bound::Included(&one), Bound::Included(&three), true)),
                Range(R::new(Bound::Included(&two), Bound::Included(&four), true)),
                Range(R::new(Bound::Included(&two), Bound::Included(&three), true)),
            ),
            (
                Range(R::new(
                    Bound::Included(&one),
                    Bound::Included(&three),
                    false,
                )),
                Range(R::new(Bound::Included(&two), Bound::Included(&four), true)),
                Range(R::new(
                    Bound::Included(&two),
                    Bound::Included(&three),
                    false,
                )),
            ),
            (
                Range(R::new(Bound::Included(&one), Bound::Excluded(&three), true)),
                Range(R::new(Bound::Included(&two), Bound::Included(&four), true)),
                Range(R::new(Bound::Included(&two), Bound::Excluded(&three), true)),
            ),
            (
                Range(R::new(
                    Bound::Included(&one),
                    Bound::Included(&three),
                    false,
                )),
                Range(R::new(Bound::Excluded(&two), Bound::Included(&four), true)),
                Range(R::new(
                    Bound::Excluded(&two),
                    Bound::Included(&three),
                    false,
                )),
            ),
            //
            // Intersecting overlapping multiple values (or multiple + range)
            // can produce either a Single or a Multiple, depending on the overlap size.
            (
                Multiple(vec![&one, &two]),
                Multiple(vec![&two, &three]),
                Single(&two),
            ),
            (
                Multiple(vec![&two, &three]),
                Multiple(vec![&one, &two]),
                Single(&two),
            ),
            (
                Multiple(vec![&two, &three]),
                Multiple(vec![&one, &two, &three, &four]),
                Multiple(vec![&two, &three]),
            ),
            (
                Multiple(vec![&one, &two]),
                Range(R::new(Bound::Included(&two), Bound::Included(&three), true)),
                Single(&two),
            ),
            (
                Multiple(vec![&two, &three]),
                Range(R::new(Bound::Included(&one), Bound::Included(&two), true)),
                Single(&two),
            ),
            (
                Multiple(vec![&two, &three]),
                Range(R::new(Bound::Included(&one), Bound::Included(&four), true)),
                Multiple(vec![&two, &three]),
            ),
            //
            // Intersecting Candidate::All from either position produces whatever the other value was.
            (All, Impossible, Impossible),
            (Impossible, All, Impossible),
            (All, Single(&one), Single(&one)),
            (Multiple(vec![&one, &two]), All, Multiple(vec![&one, &two])),
            (
                All,
                Range(R::with_start(Bound::Included(&one), true)),
                Range(R::with_start(Bound::Included(&one), true)),
            ),
            (All, All, All),
        ];

        for (original, intersected, expected) in test_cases {
            let mut base = original.clone();
            base.intersect(intersected.clone());
            assert_eq!(
                expected, base,
                "{original:?} + {intersected:?} = {base:?} != {expected:?}"
            );

            let mut base = intersected.clone();
            base.intersect(original.clone());
            assert_eq!(
                expected, base,
                "{intersected:?} + {original:?} = {base:?} != {expected:?}"
            );
        }
    }

    /// Intersecting ranges where one is completely contained in the other
    /// produces the smaller range, with appropriate "null_included".
    #[test]
    fn candidate_intersecting_preserves_overlap() {
        use CandidateValue::*;
        let one = FieldValue::Int64(1);
        let two = FieldValue::Int64(2);
        let three = FieldValue::Int64(3);
        let four = FieldValue::Int64(4);
        use super::Range as R;

        let one_incl = Bound::Included(&one);
        let one_excl = Bound::Excluded(&one);
        let four_incl = Bound::Included(&four);
        let four_excl = Bound::Excluded(&four);

        let mut larger_ranges = vec![];
        for one in [&one_incl, &one_excl, &Bound::Unbounded] {
            for four in [&four_incl, &four_excl, &Bound::Unbounded] {
                for null_included in [true, false] {
                    larger_ranges.push(Range(R::new(*one, *four, null_included)));
                }
            }
        }

        let two_incl = Bound::Included(&two);
        let two_excl = Bound::Excluded(&two);
        let three_incl = Bound::Included(&three);
        let three_excl = Bound::Excluded(&three);

        let mut smaller_ranges = vec![];
        for two in [&two_incl, &two_excl] {
            for three in [&three_incl, &three_excl] {
                for null_included in [true, false] {
                    smaller_ranges.push(Range(R::new(*two, *three, null_included)));
                }
            }
        }

        for (original, intersected) in larger_ranges
            .into_iter()
            .cartesian_product(smaller_ranges.into_iter())
        {
            let mut expected = intersected.clone();
            if let Range(r) = &mut expected {
                if let Range(r2) = &original {
                    r.null_included &= r2.null_included;
                } else {
                    unreachable!();
                }
            } else {
                unreachable!();
            }

            let mut base = original.clone();
            base.intersect(intersected.clone());
            assert_eq!(
                expected, base,
                "{original:?} + {intersected:?} = {base:?} != {expected:?}"
            );

            let mut base = intersected.clone();
            base.intersect(original.clone());
            assert_eq!(
                expected, base,
                "{intersected:?} + {original:?} = {base:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn candidate_intersecting_preserves_order_in_overlap() {
        use CandidateValue::*;
        let one = FieldValue::Int64(1);
        let two = FieldValue::Int64(2);
        let three = FieldValue::Int64(3);
        let four = FieldValue::Int64(4);
        let test_cases = [
            //
            // Intersecting multiple overlapping values preserves the order of the original.
            (
                Multiple(vec![&one, &two, &three, &four]),
                Multiple(vec![&three, &two]),
                Multiple(vec![&two, &three]),
            ),
            (
                Multiple(vec![&three, &two]),
                Multiple(vec![&one, &two, &three, &four]),
                Multiple(vec![&three, &two]),
            ),
        ];

        for (original, intersected, expected) in test_cases {
            let mut base = original.clone();
            base.intersect(intersected.clone());
            assert_eq!(
                expected, base,
                "{original:?} + {intersected:?} = {base:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn candidate_excluding_value() {
        use super::super::Range as R;
        use CandidateValue::*;
        let one = FieldValue::Int64(1);
        let two = FieldValue::Int64(2);
        let three = FieldValue::Int64(3);
        let test_data = [
            (Single(&one), &one, CandidateValue::Impossible),
            (
                // element order is preserved
                Multiple(vec![&one, &two, &three]),
                &one,
                Multiple(vec![&two, &three]),
            ),
            (
                // element order is preserved
                Multiple(vec![&one, &two, &three]),
                &two,
                Multiple(vec![&one, &three]),
            ),
            (Multiple(vec![&one, &two]), &two, Single(&one)),
            (
                Multiple(vec![&one, &FieldValue::NULL]),
                &one,
                Single(&FieldValue::NULL),
            ),
            (
                Multiple(vec![&one, &FieldValue::NULL]),
                &FieldValue::NULL,
                Single(&one),
            ),
            (Single(&one), &one, Impossible),
            (Single(&FieldValue::NULL), &FieldValue::NULL, Impossible),
            (All, &FieldValue::NULL, Range(R::full_non_null())),
            (
                Range(R::with_start(Bound::Included(&two), false)),
                &two,
                Range(R::with_start(Bound::Excluded(&two), false)),
            ),
            (
                Range(R::with_end(Bound::Included(&two), true)),
                &two,
                Range(R::with_end(Bound::Excluded(&two), true)),
            ),
            (
                Range(R::with_end(Bound::Included(&two), true)),
                &FieldValue::NULL,
                Range(R::with_end(Bound::Included(&two), false)),
            ),
        ];

        for (candidate, excluded, expected) in test_data {
            let mut base = candidate.clone();
            base.exclude_single_value(&excluded);
            assert_eq!(
                expected, base,
                "{candidate:?} - {excluded:?} produced {base:?} instead of {expected:?}"
            );
        }
    }

    #[test]
    fn candidate_excluding_value_no_ops() {
        use super::super::Range as R;
        use CandidateValue::*;
        let one = FieldValue::Int64(1);
        let two = FieldValue::Int64(2);
        let three = FieldValue::Int64(3);
        let test_data = [
            (Impossible, &one),
            (Single(&one), &two),
            (Multiple(vec![&one, &two]), &three),
            (Range(R::full_non_null()), &FieldValue::NULL),
            (Range(R::with_start(Bound::Included(&two), false)), &one),
            (Range(R::with_start(Bound::Excluded(&two), false)), &two),
            (Range(R::with_end(Bound::Included(&one), false)), &two),
            (Range(R::with_end(Bound::Excluded(&one), false)), &one),
            (
                Range(R::new(
                    Bound::Included(&one),
                    Bound::Included(&three),
                    false,
                )),
                &two,
            ),
        ];

        for (candidate, excluded) in test_data {
            let mut base = candidate.clone();
            base.exclude_single_value(&excluded);
            assert_eq!(
                candidate, base,
                "{candidate:?} - {excluded:?} should have been a no-op but it produced {base:?}"
            );
        }
    }

    #[test]
    fn candidate_excluding_value_of_different_integer_kind() {
        use super::super::Range as R;
        use CandidateValue::*;
        let signed_one = FieldValue::Int64(1);
        let signed_two = FieldValue::Int64(2);
        let unsigned_one = FieldValue::Uint64(1);
        let unsigned_two = FieldValue::Uint64(2);
        let test_data = [
            (Single(&signed_one), &unsigned_one, Impossible),
            (
                Multiple(vec![&signed_one, &signed_two]),
                &unsigned_one,
                Single(&signed_two),
            ),
            (
                Range(R::with_start(Bound::Included(&signed_one), true)),
                &unsigned_one,
                Range(R::with_start(Bound::Excluded(&signed_one), true)),
            ),
            (
                Range(R::with_end(Bound::Included(&signed_one), true)),
                &unsigned_one,
                Range(R::with_end(Bound::Excluded(&signed_one), true)),
            ),
            (Single(&unsigned_one), &signed_one, Impossible),
            (
                Multiple(vec![&unsigned_one, &unsigned_two]),
                &signed_one,
                Single(&unsigned_two),
            ),
            (
                Range(R::with_start(Bound::Included(&unsigned_one), true)),
                &signed_one,
                Range(R::with_start(Bound::Excluded(&unsigned_one), true)),
            ),
            (
                Range(R::with_end(Bound::Included(&unsigned_one), true)),
                &signed_one,
                Range(R::with_end(Bound::Excluded(&unsigned_one), true)),
            ),
        ];

        for (candidate, excluded, expected) in test_data {
            let mut base = candidate.clone();
            base.exclude_single_value(&excluded);
            assert_eq!(
                expected, base,
                "{candidate:?} - {excluded:?} produced {base:?} instead of {expected:?}"
            );
        }
    }

    #[test]
    fn candidate_direct_normalization() {
        use super::Range as R;
        use CandidateValue::*;
        let one = FieldValue::Int64(1);
        let two = FieldValue::Int64(2);
        let test_cases = [
            (Multiple(vec![]), Impossible),
            (Multiple(vec![&FieldValue::NULL]), Single(&FieldValue::NULL)),
            (Multiple(vec![&two]), Single(&two)),
            (Range(R::full()), All),
            (
                Range(R::new(Bound::Included(&one), Bound::Included(&one), true)),
                Multiple(vec![&FieldValue::NULL, &one]),
            ),
            (
                Range(R::new(Bound::Included(&one), Bound::Included(&one), false)),
                Single(&one),
            ),
            (
                Range(R::new(Bound::Included(&one), Bound::Excluded(&one), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Range(R::new(Bound::Included(&one), Bound::Excluded(&one), false)),
                Impossible,
            ),
            (
                Range(R::new(Bound::Included(&two), Bound::Included(&one), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Range(R::new(Bound::Included(&two), Bound::Included(&one), false)),
                Impossible,
            ),
            (
                Range(R::new(Bound::Excluded(&two), Bound::Included(&one), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Range(R::new(Bound::Excluded(&two), Bound::Included(&one), false)),
                Impossible,
            ),
            (
                Range(R::new(Bound::Included(&two), Bound::Excluded(&one), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Range(R::new(Bound::Included(&two), Bound::Excluded(&one), false)),
                Impossible,
            ),
            (
                Range(R::new(Bound::Excluded(&two), Bound::Excluded(&one), true)),
                Single(&FieldValue::NULL),
            ),
            (
                Range(R::new(Bound::Excluded(&two), Bound::Excluded(&one), false)),
                Impossible,
            ),
        ];

        for (unnormalized, expected) in test_cases {
            let mut base = unnormalized.clone();
            base.normalize();

            assert_eq!(
                expected, base,
                "{unnormalized:?}.normalize() = {base:?} != {expected:?}"
            );
        }
    }

    #[test]
    fn candidate_normalization() {
        use super::Range as R;
        use CandidateValue::*;
        let one = FieldValue::Int64(1);
        let two = FieldValue::Int64(2);
        let three = FieldValue::Int64(3);
        let four = FieldValue::Int64(4);
        let test_cases = [
            //
            // Causing a Multiple to lose all its elements turns it into Impossible
            (
                Multiple(vec![&one, &two, &three]),
                Single(&four),
                Impossible,
            ),
            (
                Multiple(vec![&one, &two]),
                Multiple(vec![&three, &four]),
                Impossible,
            ),
            (
                Multiple(vec![&one, &two]),
                Range(R::with_start(Bound::Included(&three), true)),
                Impossible,
            ),
            (
                Multiple(vec![&one, &two]),
                Range(R::with_start(Bound::Excluded(&two), true)),
                Impossible,
            ),
            (
                Multiple(vec![&one, &two, &FieldValue::NULL]),
                Range(R::with_start(Bound::Excluded(&two), false)),
                Impossible,
            ),
            //
            // Causing a Multiple to lose all but one of its elements turns it into Single
            (
                Multiple(vec![&one, &two, &three]),
                Single(&two),
                Single(&two),
            ),
            (
                Multiple(vec![&one, &two, &three]),
                Multiple(vec![&two, &four]),
                Single(&two),
            ),
            (
                Multiple(vec![&two, &three, &FieldValue::NULL]),
                Range(R::with_end(Bound::Included(&two), false)),
                Single(&two),
            ),
            (
                Multiple(vec![&two, &three, &FieldValue::NULL]),
                Range(R::with_end(Bound::Excluded(&two), true)),
                Single(&FieldValue::NULL),
            ),
        ];

        for (original, intersected, expected) in test_cases {
            let mut base = original.clone();
            base.intersect(intersected.clone());
            assert_eq!(
                expected, base,
                "{original:?} + {intersected:?} = {base:?} != {expected:?}"
            );

            let mut base = intersected.clone();
            base.intersect(original.clone());
            assert_eq!(
                expected, base,
                "{intersected:?} + {original:?} = {base:?} != {expected:?}"
            );
        }
    }

    /// The test cases here codify the current behavior, which isn't that smart about integers.
    /// If/when the code becomes smarter here, these test cases can be updated and moved out
    /// from this module.
    mod future_work {
        use std::ops::Bound;

        use crate::{interpreter::hints::CandidateValue, ir::FieldValue};

        fn range_normalization_does_not_prefer_inclusive_bounds_for_integers() {
            use super::super::Range as R;
            use CandidateValue::*;
            let signed_one = FieldValue::Int64(1);
            let signed_three = FieldValue::Int64(3);
            let unsigned_one = FieldValue::Uint64(1);
            let unsigned_three = FieldValue::Uint64(3);
            let test_data = [
                Range(R::new(
                    Bound::Excluded(&signed_one),
                    Bound::Excluded(&signed_three),
                    false,
                )),
                Range(R::new(
                    Bound::Excluded(&unsigned_one),
                    Bound::Excluded(&unsigned_three),
                    false,
                )),
                Range(R::with_start(Bound::Excluded(&signed_one), false)),
                Range(R::with_start(Bound::Excluded(&unsigned_one), false)),
                Range(R::with_end(Bound::Excluded(&signed_three), false)),
                Range(R::with_end(Bound::Excluded(&unsigned_three), false)),
            ];

            for candidate in test_data {
                let mut base = candidate.clone();
                base.normalize();
                assert_eq!(
                    candidate, base,
                    "normalization changed this value: {candidate:?} != {base:?}"
                );
            }
        }

        fn candidate_value_exclusion_does_not_special_case_integers() {
            use super::super::Range as R;
            use CandidateValue::*;
            let signed_one = FieldValue::Int64(1);
            let signed_two = FieldValue::Int64(2);
            let signed_three = FieldValue::Int64(3);
            let unsigned_one = FieldValue::Uint64(1);
            let unsigned_two = FieldValue::Uint64(2);
            let unsigned_three = FieldValue::Uint64(3);
            let test_data = [
                (Range(R::full_non_null()), &FieldValue::Int64(i64::MIN)),
                (Range(R::full_non_null()), &FieldValue::Int64(i64::MAX)),
                (Range(R::full_non_null()), &FieldValue::Uint64(u64::MIN)),
                (Range(R::full_non_null()), &FieldValue::Uint64(u64::MAX)),
                (
                    Range(R::with_start(Bound::Excluded(&signed_one), false)),
                    &signed_two,
                ),
                (
                    Range(R::with_start(Bound::Excluded(&unsigned_one), false)),
                    &unsigned_two,
                ),
                (
                    Range(R::with_end(Bound::Excluded(&signed_two), false)),
                    &signed_one,
                ),
                (
                    Range(R::with_end(Bound::Excluded(&unsigned_two), false)),
                    &unsigned_one,
                ),
                (
                    Range(R::new(
                        Bound::Included(&unsigned_one),
                        Bound::Included(&unsigned_three),
                        false,
                    )),
                    &unsigned_two,
                ),
                (
                    Range(R::new(
                        Bound::Included(&signed_one),
                        Bound::Included(&signed_three),
                        false,
                    )),
                    &signed_two,
                ),
            ];

            for (candidate, excluded) in test_data {
                let mut base = candidate.clone();
                base.exclude_single_value(&excluded);
                assert_eq!(
                    candidate, base,
                    "exclusion changed this value: {candidate:?} - {excluded:?} produced {base:?}"
                );
            }
        }
    }
}
