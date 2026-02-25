use std::fmt::Debug;
use std::ops::{Add, Deref, Range, Sub};

/// NonZero is only checked during Debug and should not be relied upon for safety
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct NonZeroRange<T>(RangeUnchecked<T>);

impl<T: Debug> Debug for NonZeroRange<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}..{:?}", self.start, self.end))
    }
}

/// Exists, because std::ops::Range is not Copy
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RangeUnchecked<T> {
    pub start: T,
    pub end: T,
}

impl<T> From<NonZeroRange<T>> for std::ops::Range<T> {
    fn from(value: NonZeroRange<T>) -> Self {
        value.0.start..value.0.end
    }
}
impl<T: PartialOrd> TryFrom<std::ops::Range<T>> for NonZeroRange<T> {
    type Error = RangeZeroLenghtError<T>;

    fn try_from(value: std::ops::Range<T>) -> Result<Self, Self::Error> {
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

impl<T: PartialOrd + Debug> NonZeroRange<T> {
    pub fn new(range: Range<T>) -> Self {
        let r = Self(RangeUnchecked {
            start: range.start,
            end: range.end,
        });
        assert!(
            r.start < r.end,
            "NonZeroRange must contain a element: {:?}",
            r
        );
        r
    }
    /// Safety
    /// range.start has to be < range.end
    pub unsafe fn new_unchecked(range: Range<T>) -> Self {
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

    pub fn len(&self) -> T
    where
        T: Sub<Output = T> + Copy,
    {
        self.end - self.start
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl<T: Add<Output = T> + Copy> Add<T> for NonZeroRange<T> {
    type Output = NonZeroRange<T>;

    fn add(self, rhs: T) -> Self::Output {
        NonZeroRange(RangeUnchecked {
            start: self.start + rhs,
            end: self.end + rhs,
        })
    }
}

impl<T: Sub<Output = T> + Copy> Sub<T> for NonZeroRange<T> {
    type Output = NonZeroRange<T>;

    fn sub(self, rhs: T) -> Self::Output {
        NonZeroRange(RangeUnchecked {
            start: self.start - rhs,
            end: self.end - rhs,
        })
    }
}

impl<T> Deref for NonZeroRange<T> {
    type Target = RangeUnchecked<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{0:?} is empty")]
pub struct RangeZeroLenghtError<T>(std::ops::Range<T>);

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
        assert_eq!(expected, unsafe {
            NonZeroRange::new_unchecked(a.clone()).overlaps(&NonZeroRange::new_unchecked(b.clone()))
        });
        assert_eq!(expected, unsafe {
            NonZeroRange::new_unchecked(b).overlaps(&NonZeroRange::new_unchecked(a))
        });
    }
}
