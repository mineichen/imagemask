use std::{
    cmp::min,
    iter::FusedIterator,
    num::NonZeroU32,
    ops::{Add, Div, Mul, Rem, Sub},
};

use crate::{CreateRange, ImageDimension, SignedNonZeroable, UncheckedCast};
start
pub struct SortedRangesIterGlobal<TIncludedIter, TExcludedIter, TOut: CreateRange> {
    included: TIncludedIter,
    excluded: TExcludedIter,
    accumulator: TOut::Item,
    col_offset: Option<TOut::Item>,
    new_width: NonZeroU32,
    old_width: TOut::Item,
    new_width_out: TOut::Item,
}

impl<TIncludedIter, TExcludedIter, TRange: CreateRange>
    SortedRangesIterGlobal<TIncludedIter, TExcludedIter, TRange>
where
    u32: UncheckedCast<TRange::Item>,
    TRange::Item: Copy,
{
    pub(crate) fn new(
        included: TIncludedIter,
        excluded: TExcludedIter,
        accumulator: TRange::Item,
        old_width: NonZeroU32,
        new_width: NonZeroU32,
    ) -> Self {
        Self {
            included,
            excluded,
            accumulator,
            col_offset: None,
            new_width,
            old_width: old_width.get().cast_unchecked(),
            new_width_out: new_width.get().cast_unchecked(),
        }
    }
}

impl<TIncludedIter, TExcludedIter, TOut: CreateRange> ImageDimension
    for SortedRangesIterGlobal<TIncludedIter, TExcludedIter, TOut>
{
    fn width(&self) -> std::num::NonZero<u32> {
        self.new_width
    }
}

impl<TIncluded, TExcluded, TOut> Iterator for SortedRangesIterGlobal<TIncluded, TExcluded, TOut>
where
    TIncluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TExcluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TOut: CreateRange<
        Item: Copy
                  + SignedNonZeroable
                  + Add<Output = TOut::Item>
                  + Sub<Output = TOut::Item>
                  + Mul<Output = TOut::Item>
                  + Div<Output = TOut::Item>
                  + Rem<Output = TOut::Item>
                  + Ord,
    >,
{
    type Item = TOut;

    fn next(&mut self) -> Option<Self::Item> {
        let exclude = self.excluded.next()?.cast_unchecked();
        let include = self.included.next()?.cast_unchecked();

        let col_offset = match self.col_offset {
            Some(co) => co,
            None => {
                let co = exclude % self.old_width;
                let row_offset = exclude / self.old_width;
                self.accumulator = self.new_width_out * row_offset + co;
                self.col_offset = Some(co);
                co
            }
        };

        let range_width = min(include, self.new_width_out - col_offset);
        let start = self.accumulator;
        let end = start + range_width;
        self.accumulator = self.accumulator + self.new_width_out;

        Some(TOut::new_debug_checked_zeroable(start, end))
    }
}

impl<TIncluded, TExcluded, TOut> FusedIterator
    for SortedRangesIterGlobal<TIncluded, TExcluded, TOut>
where
    TIncluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TExcluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TOut: CreateRange<
        Item: Copy
                  + SignedNonZeroable
                  + Add<Output = TOut::Item>
                  + Sub<Output = TOut::Item>
                  + Mul<Output = TOut::Item>
                  + Div<Output = TOut::Item>
                  + Rem<Output = TOut::Item>
                  + Ord,
    >,
{
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_0_5_interop {
    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};
    use std::ops::RangeInclusive;

    use super::*;

    impl<TIncluded, TExcluded, T> SortedStarts<T>
        for SortedRangesIterGlobal<TIncluded, TExcluded, RangeInclusive<T>>
    where
        TIncluded: FusedIterator<Item: UncheckedCast<T>>,
        TExcluded: Iterator<Item: UncheckedCast<T>>,
        T: Copy
            + SignedNonZeroable
            + std::ops::Add<Output = T>
            + std::ops::Sub<Output = T>
            + std::ops::Mul<Output = T>
            + std::ops::Div<Output = T>
            + std::ops::Rem<Output = T>
            + Ord
            + num_traits::One
            + Integer,
    {
    }
    impl<TIncluded, TExcluded, T> SortedDisjoint<T>
        for SortedRangesIterGlobal<TIncluded, TExcluded, RangeInclusive<T>>
    where
        SortedRangesIterGlobal<TIncluded, TExcluded, RangeInclusive<T>>: SortedStarts<T>,
        T: Copy
            + SignedNonZeroable
            + std::ops::Add<Output = T>
            + std::ops::Sub<Output = T>
            + std::ops::Mul<Output = T>
            + std::ops::Div<Output = T>
            + std::ops::Rem<Output = T>
            + Ord
            + num_traits::One
            + Integer,
    {
    }
}
