use std::fmt::Debug;

use crate::ir::FieldValue;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CandidateValue<T> {
    Impossible,       // statically determined that no values fit
    Single(T),        // there's only one value that fits
    Multiple(Vec<T>), // there are multiple values that fit
    All,              // all values are possible for this specific entry
                      // (e.g. used for tag-from-non-existent-optional cases)
}

impl<T: Debug + Clone + PartialEq + Eq> CandidateValue<T> {
    pub(super) fn merge(&mut self, other: CandidateValue<T>) {
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
                    Self::Multiple(others) => {
                        multiple.retain(|value| others.contains(value));
                        let possibilities = multiple.len();
                        if possibilities == 0 {
                            *self = Self::Impossible;
                        } else if possibilities == 1 {
                            let first = multiple.swap_remove(0);
                            *self = Self::Single(first);
                        } // otherwise it stays Multiple and we already mutated the Vec it holds
                    }
                    Self::All => {} // self is unchanged.
                }
            }
            Self::All => {
                // Whatever the other candidate was. It can't be any wider than Self::All.
                *self = other;
            }
        }
    }
}

// TODO: should we just use Rust's built in range types?
//       is there an advantage one way or the other?
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeBoundKind {
    Lower(RangeEndpoint),
    Upper(RangeEndpoint),
    LowerAndUpper(RangeEndpoint, RangeEndpoint),
}

impl RangeBoundKind {
    pub fn start(&self) -> Option<&RangeEndpoint> {
        match self {
            RangeBoundKind::Lower(l) => Some(l),
            RangeBoundKind::Upper(_) => None,
            RangeBoundKind::LowerAndUpper(l, _) => Some(l),
        }
    }

    pub fn end(&self) -> Option<&RangeEndpoint> {
        match self {
            RangeBoundKind::Lower(_) => None,
            RangeBoundKind::Upper(r) => Some(r),
            RangeBoundKind::LowerAndUpper(_, r) => Some(r),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangeEndpoint {
    Exclusive(FieldValue),
    Inclusive(FieldValue),
}

#[cfg(test)]
mod tests {
    use crate::ir::FieldValue;

    use super::CandidateValue;

    #[test]
    fn test_candidate_merging() {
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
            //
            // Merging Impossible into anything produces Imposssible.
            (Single(&one), Impossible, Impossible),
            (Multiple(vec![&one, &two]), Impossible, Impossible),
            //
            // Merging null into non-null, or vice versa, produces Impossible.
            (Single(&FieldValue::NULL), Single(&one), Impossible),
            (
                Single(&FieldValue::NULL),
                Multiple(vec![&one, &two]),
                Impossible,
            ),
            (Single(&one), Single(&FieldValue::NULL), Impossible),
            (
                Multiple(vec![&one, &two]),
                Single(&FieldValue::NULL),
                Impossible,
            ),
            //
            // Merging non-overlapping single or multiple values produces Impossible.
            (Single(&one), Single(&two), Impossible),
            (Single(&one), Multiple(vec![&two, &three]), Impossible),
            (Multiple(vec![&one, &two]), Single(&three), Impossible),
            (
                Multiple(vec![&one, &two]),
                Multiple(vec![&three, &four]),
                Impossible,
            ),
            //
            // Merging overlapping single values, or single with multiple,
            // produces the overlapping Single.
            (Single(&one), Single(&one), Single(&one)),
            (Multiple(vec![&one, &two]), Single(&one), Single(&one)),
            (Single(&one), Multiple(vec![&one, &two]), Single(&one)),
            //
            // Merging null into multiple that contains null produces null.
            (
                Single(&FieldValue::NULL),
                Multiple(vec![&one, &FieldValue::Null]),
                Single(&FieldValue::NULL),
            ),
            //
            // Merging multiple values that include null works correctly too.
            (
                Multiple(vec![&one, &FieldValue::Null, &two, &three]),
                Multiple(vec![&one, &FieldValue::Null, &four]),
                Multiple(vec![&one, &FieldValue::Null]),
            ),
            //
            // Merging overlapping multiple values can produce either a Single or a Multiple,
            // depending on the overlap size.
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
                Multiple(vec![&one, &two, &three, &four]),
                Multiple(vec![&two, &three]),
                Multiple(vec![&two, &three]),
            ),
            (
                Multiple(vec![&two, &three]),
                Multiple(vec![&one, &two, &three, &four]),
                Multiple(vec![&two, &three]),
            ),
            //
            // Merging multiple overlapping values preserves the order of the original.
            (
                Multiple(vec![&one, &two, &three, &four]),
                Multiple(vec![&three, &two]),
                Multiple(vec![&two, &three]),
            ),
            //
            // Merging Candidate::All from either position produces whatever the other value was.
            (All, Impossible, Impossible),
            (Impossible, All, Impossible),
            (All, Single(&one), Single(&one)),
            (Single(&one), All, Single(&one)),
            (All, Multiple(vec![&one, &two]), Multiple(vec![&one, &two])),
            (Multiple(vec![&one, &two]), All, Multiple(vec![&one, &two])),
            (All, All, All),
        ];

        for (original, merged, expected) in test_cases {
            let mut base = original.clone();
            base.merge(merged.clone());
            assert_eq!(
                expected, base,
                "{original:?} + {merged:?} = {base:?} != {expected:?}"
            );
        }
    }
}
