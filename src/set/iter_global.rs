use std::{
    cmp::min,
    iter::FusedIterator,
    num::NonZeroU32,
    ops::{Add, Div, Mul, Rem, Sub},
};

use crate::{CreateRange, ImageDimension, SignedNonZeroable, UncheckedCast};

pub struct SortedRangesIterGlobal<I, E, T: CreateRange> {
    included: I,
    excluded: E,
    pos: T::Item,
    remaining: T::Item,
    old_width: T::Item,
    new_width_out: T::Item,
    new_width: NonZeroU32,
}

impl<I, E, T: CreateRange> SortedRangesIterGlobal<I, E, T>
where
    u32: UncheckedCast<T::Item>,
    T::Item: Copy + Default,
{
    pub(crate) fn new(
        included: I,
        excluded: E,
        old_width: NonZeroU32,
        new_width: NonZeroU32,
    ) -> Self {
        Self {
            included,
            excluded,
            pos: T::Item::default(),
            remaining: T::Item::default(),
            old_width: old_width.get().cast_unchecked(),
            new_width,
            new_width_out: new_width.get().cast_unchecked(),
        }
    }
}

impl<I, E, T: CreateRange> ImageDimension for SortedRangesIterGlobal<I, E, T> {
    fn width(&self) -> std::num::NonZero<u32> {
        self.new_width
    }
}

impl<TI, TE, TOut> Iterator for SortedRangesIterGlobal<TI, TE, TOut>
where
    TI: Iterator<Item: UncheckedCast<TOut::Item>>,
    TE: Iterator<Item: UncheckedCast<TOut::Item>>,
    TOut: CreateRange<
        Item: Copy
                  + Default
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
        let zero = TOut::Item::default();
        loop {
            if self.remaining > zero {
                let col = self.pos % self.old_width;
                let row = self.pos / self.old_width;
                let take = min(self.remaining, self.old_width - col);
                self.pos = self.pos + take;
                self.remaining = self.remaining - take;
                let s = row * self.new_width_out + col;
                return Some(TOut::new_debug_checked_zeroable(s, s + take));
            }
            self.pos = self.pos + self.excluded.next()?.cast_unchecked();
            let include = self.included.next()?.cast_unchecked();
            if self.old_width >= self.new_width_out {
                let end = self.pos + include;
                let new = |p: TOut::Item| {
                    (p / self.old_width) * self.new_width_out
                        + min(p % self.old_width, self.new_width_out)
                };
                let (s, e) = (new(self.pos), new(end));
                self.pos = end;
                if s < e {
                    return Some(TOut::new_debug_checked_zeroable(s, e));
                }
            } else {
                self.remaining = include;
            }
        }
    }
}

impl<TI, TE, TOut: CreateRange> FusedIterator for SortedRangesIterGlobal<TI, TE, TOut> where
    Self: Iterator
{
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_0_5_interop {
    use super::*;
    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};
    use std::ops::RangeInclusive;

    impl<TI, TE, T> SortedStarts<T> for SortedRangesIterGlobal<TI, TE, RangeInclusive<T>>
    where
        TI: FusedIterator<Item: UncheckedCast<T>>,
        TE: Iterator<Item: UncheckedCast<T>>,
        T: Copy
            + Default
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
    impl<TI, TE, T> SortedDisjoint<T> for SortedRangesIterGlobal<TI, TE, RangeInclusive<T>>
    where
        SortedRangesIterGlobal<TI, TE, RangeInclusive<T>>: SortedStarts<T>,
        T: Copy
            + Default
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
