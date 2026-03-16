use std::{iter::FusedIterator, marker::PhantomData};

use crate::{CreateRange, SignedNonZeroable, UncheckedCast};

pub struct SortedRangesIter<TIncludedIter, TExcludedIter, TOut: CreateRange> {
    include: TIncludedIter,
    excluded: TExcludedIter,
    offset: TOut::Item,
    _out: PhantomData<TOut>,
}

impl<TIncludedIter, TExcludedIter, TRange: CreateRange>
    SortedRangesIter<TIncludedIter, TExcludedIter, TRange>
{
    pub(crate) fn new(
        include: TIncludedIter,
        excluded: TExcludedIter,
        offset: TRange::Item,
    ) -> Self {
        Self {
            include,
            excluded,
            offset,
            _out: PhantomData,
        }
    }
}

impl<TIncluded, TExcluded, TOut> Iterator for SortedRangesIter<TIncluded, TExcluded, TOut>
where
    TIncluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TExcluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TOut: CreateRange<Item: Copy + SignedNonZeroable + std::ops::Add<Output = TOut::Item>>,
{
    type Item = TOut;

    fn next(&mut self) -> Option<Self::Item> {
        let exclude = self.excluded.next()?.cast_unchecked();
        self.offset = self.offset + exclude;

        let include = self.include.next()?.cast_unchecked();
        let out_range = TOut::new_debug_checked(self.offset, include.create_non_zero().unwrap());
        self.offset = self.offset + include;

        Some(out_range)
    }
}

impl<TIncluded, TExcluded, TOut> FusedIterator for SortedRangesIter<TIncluded, TExcluded, TOut>
where
    TIncluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TExcluded: Iterator<Item: UncheckedCast<TOut::Item>>,
    TOut: CreateRange<Item: Copy + SignedNonZeroable + std::ops::Add<Output = TOut::Item>>,
{
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_0_5_interop {
    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};
    use std::ops::RangeInclusive;

    use super::*;

    impl<TIncluded, TExcluded, T> SortedStarts<T>
        for SortedRangesIter<TIncluded, TExcluded, RangeInclusive<T>>
    where
        TIncluded: FusedIterator<Item: UncheckedCast<T>>,
        TExcluded: Iterator<Item: UncheckedCast<T>>,
        T: Copy
            + SignedNonZeroable
            + std::ops::Add<Output = T>
            + std::ops::Sub<Output = T>
            + num_traits::One
            + Integer,
    {
    }
    impl<TIncluded, TExcluded, T> SortedDisjoint<T>
        for SortedRangesIter<TIncluded, TExcluded, RangeInclusive<T>>
    where
        TIncluded: FusedIterator<Item: UncheckedCast<T>>,
        TExcluded: Iterator<Item: UncheckedCast<T>>,
        T: Copy
            + SignedNonZeroable
            + std::ops::Add<Output = T>
            + std::ops::Sub<Output = T>
            + num_traits::One
            + Integer,
    {
    }
}
