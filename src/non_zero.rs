use std::cmp::{max, min};
use std::fmt::Debug;
use std::num::NonZero;
use std::ops::{Add, Deref, Range, RangeInclusive, Sub};

use num_traits::{Bounded, One};

/// NonZero is only checked during Debug and should not be relied upon for safety
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct NonZeroRange<T>(RangeUnchecked<T>);

impl<T: Debug> Debug for NonZeroRange<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{:?}..{:?}", self.start, self.end))
    }
}

/// Exists, because std::ops::Range is not Copy
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RangeUnchecked<T> {
    pub start: T,
    pub end: T,
}

impl NonZeroRange<u64> {
    pub fn with_offset(&self, offset: i64) -> Self {
        NonZeroRange(RangeUnchecked {
            start: self.0.start.strict_add_signed(offset),
            end: self.0.end.strict_add_signed(offset),
        })
    }

    pub fn increment_length(&mut self) {
        self.0.end = self.0.end.checked_add(1).expect("Never overflows");
    }
}

impl<T> From<NonZeroRange<T>> for std::ops::Range<T> {
    fn from(value: NonZeroRange<T>) -> Self {
        value.0.start..value.0.end
    }
}
impl<T: PartialOrd> TryFrom<std::ops::Range<T>> for NonZeroRange<T> {
    type Error = RangeZeroLenghtError<std::ops::Range<T>>;

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

impl<T: Sub<Output = T> + One> From<NonZeroRange<T>> for std::ops::RangeInclusive<T> {
    fn from(value: NonZeroRange<T>) -> Self {
        value.0.start..=value.0.end - T::one()
    }
}
impl<T: PartialOrd + Add<Output = T> + One> TryFrom<std::ops::RangeInclusive<T>>
    for NonZeroRange<T>
{
    type Error = RangeZeroLenghtError<RangeInclusive<T>>;

    fn try_from(value: std::ops::RangeInclusive<T>) -> Result<Self, Self::Error> {
        if value.is_empty() {
            Err(RangeZeroLenghtError(value))
        } else {
            let (start, end) = value.into_inner();
            let end = end + T::one();
            Ok(NonZeroRange(RangeUnchecked { start, end }))
        }
    }
}

pub trait SignedNonZeroable: Sized {
    type NonZero: Into<Self> + Copy;
    fn add_nonzero(self, other: Self::NonZero) -> Self;
    fn create_non_zero(self) -> Option<Self::NonZero>;

    /// # Safety
    /// Provided value mustn't be 0
    unsafe fn create_non_zero_unchecked(self) -> Self::NonZero;
}

impl SignedNonZeroable for u8 {
    type NonZero = NonZero<u8>;

    #[inline]
    fn add_nonzero(self, other: Self::NonZero) -> Self {
        self + other.get()
    }

    #[inline]
    fn create_non_zero(self) -> Option<Self::NonZero> {
        NonZero::new(self)
    }

    #[inline]
    unsafe fn create_non_zero_unchecked(self) -> Self::NonZero {
        unsafe { NonZero::new_unchecked(self) }
    }
}
impl SignedNonZeroable for u16 {
    type NonZero = NonZero<u16>;

    #[inline]
    fn add_nonzero(self, other: Self::NonZero) -> Self {
        self + other.get()
    }

    #[inline]
    fn create_non_zero(self) -> Option<Self::NonZero> {
        NonZero::new(self)
    }

    #[inline]
    unsafe fn create_non_zero_unchecked(self) -> Self::NonZero {
        unsafe { NonZero::new_unchecked(self) }
    }
}
impl SignedNonZeroable for u32 {
    type NonZero = NonZero<u32>;

    #[inline]
    fn add_nonzero(self, other: Self::NonZero) -> Self {
        self + other.get()
    }

    #[inline]
    fn create_non_zero(self) -> Option<Self::NonZero> {
        NonZero::new(self)
    }

    #[inline]
    unsafe fn create_non_zero_unchecked(self) -> Self::NonZero {
        unsafe { NonZero::new_unchecked(self) }
    }
}
impl SignedNonZeroable for u64 {
    type NonZero = NonZero<u64>;

    #[inline]
    fn add_nonzero(self, other: Self::NonZero) -> Self {
        self + other.get()
    }

    #[inline]
    fn create_non_zero(self) -> Option<Self::NonZero> {
        NonZero::new(self)
    }

    #[inline]
    unsafe fn create_non_zero_unchecked(self) -> Self::NonZero {
        unsafe { NonZero::new_unchecked(self) }
    }
}

impl SignedNonZeroable for usize {
    type NonZero = NonZero<usize>;

    #[inline]
    fn add_nonzero(self, other: Self::NonZero) -> Self {
        self + other.get()
    }

    #[inline]
    fn create_non_zero(self) -> Option<Self::NonZero> {
        NonZero::new(self)
    }

    #[inline]
    unsafe fn create_non_zero_unchecked(self) -> Self::NonZero {
        unsafe { NonZero::new_unchecked(self) }
    }
}

impl<T> From<Range<T>> for RangeUnchecked<T> {
    fn from(value: Range<T>) -> Self {
        RangeUnchecked {
            start: value.start,
            end: value.end,
        }
    }
}

impl<T: One + Sub<Output = T> + Add<Output = T>> From<RangeInclusive<T>> for RangeUnchecked<T> {
    fn from(value: RangeInclusive<T>) -> Self {
        let (start, end) = value.into_inner();
        RangeUnchecked {
            start,
            end: end - T::one(),
        }
    }
}

impl<T> NonZeroRange<T> {
    #[inline]
    pub fn from_span(start: T, len: T::NonZero) -> Self
    where
        T: Copy + SignedNonZeroable,
    {
        let end = start.add_nonzero(len);
        Self(RangeUnchecked { start, end })
    }
}

impl<T: PartialOrd + Ord + Copy + Debug + Bounded> NonZeroRange<T> {
    pub fn new(into_range: impl Into<RangeUnchecked<T>>) -> Self {
        let r = Self(into_range.into());
        assert!(
            r.start < r.end,
            "NonZeroRange must contain a element: {:?}",
            r
        );
        r
    }
    /// # Safety
    /// range.start has to be < range.end
    pub unsafe fn new_unchecked(into_range: impl Into<RangeUnchecked<T>>) -> Self {
        let r = Self(into_range.into());
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
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let start = max(self.start, other.start);
        let end = min(self.end, other.end);
        if end > start {
            Some(unsafe { Self::new_unchecked(RangeUnchecked { start, end }) })
        } else {
            None
        }
    }
}
impl<T> NonZeroRange<T>
where
    T: Sub<Output = T> + Copy,
{
    pub fn len(&self) -> T {
        self.end - self.start
    }

    pub fn len_non_zero(&self) -> T::NonZero
    where
        T: SignedNonZeroable,
    {
        // We don't check every numeric operation, so len could be zero, but this is UB already
        T::create_non_zero(self.end - self.start).expect("A operation probably overflowed")
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
pub struct RangeZeroLenghtError<T>(T);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_overlapping_adjacent() {
        test_both_way_overlap(0..5, 5..10, false);
    }

    #[test]
    fn overlapping() {
        test_both_way_overlap(0..5, 3..7, true);
    }

    #[test]
    fn one_inside_other() {
        test_both_way_overlap(2..4, 1..5, true);
    }

    #[test]
    fn same_ranges() {
        test_both_way_overlap(3..7, 3..7, true);
    }

    #[test]
    fn completely_separate() {
        test_both_way_overlap(0..2, 3..5, false);
    }

    #[test]
    fn overlapping_start() {
        test_both_way_overlap(0..5, 4..6, true);
    }

    #[test]
    fn overlapping_end() {
        test_both_way_overlap(3..7, 0..4, true);
    }
    fn test_both_way_overlap(a: Range<u32>, b: Range<u32>, expected: bool) {
        assert_eq!(expected, unsafe {
            NonZeroRange::new_unchecked(a.clone()).overlaps(&NonZeroRange::new_unchecked(b.clone()))
        });
        assert_eq!(expected, unsafe {
            NonZeroRange::new_unchecked(b).overlaps(&NonZeroRange::new_unchecked(a))
        });
    }

    #[test]
    fn intersection_no_overlap_before() {
        test_intersection_both_ways(0..2, 3..5, None);
    }

    #[test]
    fn intersection_adjacent() {
        test_intersection_both_ways(0..5, 5..10, None);
    }

    #[test]
    fn intersection_overlaping_start() {
        test_intersection_both_ways(0..5, 3..7, Some(3..5));
    }

    #[test]
    fn intersection_one_inside_other() {
        test_intersection_both_ways(2..4, 1..5, Some(2..4));
    }

    #[test]
    fn intersection_same_ranges() {
        test_intersection_both_ways(3..7, 3..7, Some(3..7));
    }

    #[test]
    fn intersection_overlapping_end() {
        test_intersection_both_ways(3..7, 0..4, Some(3..4));
    }

    fn test_intersection_both_ways(a: Range<u32>, b: Range<u32>, expected: Option<Range<u32>>) {
        let a_nz = unsafe { NonZeroRange::new_unchecked(a.clone()) };
        let b_nz = unsafe { NonZeroRange::new_unchecked(b.clone()) };

        let result_ab = a_nz.intersection(&b_nz);
        let result_ba = b_nz.intersection(&a_nz);

        match expected {
            Some(exp) => {
                let exp_nz = unsafe { NonZeroRange::new_unchecked(exp) };
                assert_eq!(result_ab, Some(exp_nz), "a intersection b");
                assert_eq!(result_ba, Some(exp_nz), "b intersection a");
            }
            None => {
                assert_eq!(result_ab, None, "a intersection b");
                assert_eq!(result_ba, None, "b intersection a");
            }
        }
    }
}
