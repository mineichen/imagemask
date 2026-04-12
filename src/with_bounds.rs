use std::{iter::FusedIterator, num::NonZero};

pub trait ImageDimension {
    fn width(&self) -> NonZero<u32>;
}

#[derive(Clone, Debug)]
pub struct WithBounds<I> {
    iter: I,
    width: NonZero<u32>,
}

impl<I> WithBounds<I> {
    pub fn new(iter: impl IntoIterator<IntoIter = I>, width: NonZero<u32>) -> Self {
        Self {
            iter: iter.into_iter(),
            width,
        }
    }

    pub fn into_inner(self) -> I {
        self.iter
    }
}

impl<I: Iterator> Iterator for WithBounds<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<I: FusedIterator> FusedIterator for WithBounds<I> {}

impl<I> ImageDimension for WithBounds<I> {
    fn width(&self) -> NonZero<u32> {
        self.width
    }
}

#[cfg(feature = "range-set-blaze-0_5")]
mod with_bounds_range_set_blaze_0_5 {
    use super::*;
    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};

    impl<T, TRangeItem> SortedStarts<TRangeItem> for WithBounds<T>
    where
        T: SortedStarts<TRangeItem>,
        TRangeItem: Integer,
    {
    }
    impl<T, TRangeItem> SortedDisjoint<TRangeItem> for WithBounds<T>
    where
        T: SortedDisjoint<TRangeItem>,
        TRangeItem: Integer,
    {
    }
}
