use std::fmt::Debug;
use std::ops::{Deref, Range};

/// NonZero is only checked during Debug and should not be relied upon for safety
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NonZeroRange(RangeUnchecked);

impl Debug for NonZeroRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}..{:?}", self.start, self.end))
    }
}

/// Exists, because std::ops::Range is not Copy
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RangeUnchecked {
    pub start: u32,
    pub end: u32,
}

impl From<NonZeroRange> for std::ops::Range<u32> {
    fn from(value: NonZeroRange) -> Self {
        value.0.start..value.0.end
    }
}
impl TryFrom<std::ops::Range<u32>> for NonZeroRange {
    type Error = RangeZeroLenghtError;

    fn try_from(value: std::ops::Range<u32>) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(RangeZeroLenghtError(value))
        } else {
            Ok(NonZeroRange(RangeUnchecked {
                start: value.start,
                end: value.end,
            }))
        }
    }
}

impl NonZeroRange {
    pub fn new_unchecked(range: Range<u32>) -> Self {
        let r = Self(RangeUnchecked {
            start: range.start,
            end: range.end,
        });
        debug_assert!(
            r.start < r.end,
            "NonZeroRange must contain a element: {:?}",
            r
        );
        r
    }

    pub fn overlaps(&self, other: &Self) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl std::ops::Add<u32> for NonZeroRange {
    type Output = NonZeroRange;

    fn add(self, rhs: u32) -> Self::Output {
        NonZeroRange(RangeUnchecked {
            start: self.start + rhs,
            end: self.end + rhs,
        })
    }
}

impl std::ops::Sub<u32> for NonZeroRange {
    type Output = NonZeroRange;

    fn sub(self, rhs: u32) -> Self::Output {
        NonZeroRange(RangeUnchecked {
            start: self.start - rhs,
            end: self.end - rhs,
        })
    }
}

impl Deref for NonZeroRange {
    type Target = RangeUnchecked;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0:?} is empty")]
pub struct RangeZeroLenghtError(std::ops::Range<u32>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_overlapping_adjacent() {
        test_both_way(0..5, 5..10, false);
    }

    #[test]
    fn overlapping() {
        test_both_way(0..5, 3..7, true);
    }

    #[test]
    fn one_inside_other() {
        test_both_way(2..4, 1..5, true);
    }

    #[test]
    fn same_ranges() {
        test_both_way(3..7, 3..7, true);
    }

    #[test]
    fn completely_separate() {
        test_both_way(0..2, 3..5, false);
    }

    #[test]
    fn overlapping_start() {
        test_both_way(0..5, 4..6, true);
    }

    #[test]
    fn overlapping_end() {
        test_both_way(3..7, 0..4, true);
    }
    fn test_both_way(a: Range<u32>, b: Range<u32>, expected: bool) {
        assert_eq!(
            expected,
            NonZeroRange::new_unchecked(a.clone())
                .overlaps(&NonZeroRange::new_unchecked(b.clone()))
        );
        assert_eq!(
            expected,
            NonZeroRange::new_unchecked(b).overlaps(&NonZeroRange::new_unchecked(a))
        );
    }
}
