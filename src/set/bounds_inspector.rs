use std::{iter::FusedIterator, marker::PhantomData, num::NonZero};

use num_traits::{Bounded, One, Zero};

use crate::{CreateRange, ImageDimension, Rect, SignedNonZeroable};

#[cfg(feature = "range-set-blaze-0_5")]
use std::ops::RangeInclusive;

pub struct BoundsInspector<T, R: CreateRange> {
    parent: T,
    width: <R::Item as SignedNonZeroable>::NonZero,
    _range: PhantomData<R>,
    min_column: R::Item,
    max_column: R::Item,
    min_row: R::Item,
    max_row: R::Item,
}

impl<T, R> BoundsInspector<T, R>
where
    T: Iterator,
    R: CreateRange,
    R::Item: Bounded
        + Copy
        + Ord
        + Zero
        + One
        + std::ops::Add<Output = R::Item>
        + std::ops::Sub<Output = R::Item>
        + std::ops::Rem<Output = R::Item>
        + std::ops::Div<Output = R::Item>,
{
    pub fn new(parent: T, width: <R::Item as SignedNonZeroable>::NonZero) -> Self {
        BoundsInspector {
            parent,
            width,
            _range: PhantomData,
            min_column: R::Item::max_value(),
            max_column: R::Item::min_value(),
            min_row: R::Item::max_value(),
            max_row: R::Item::min_value(),
        }
    }

    pub fn bounds(&self) -> Option<Rect<R::Item>> {
        if self.max_row < self.min_row {
            return None;
        }

        let width = self.max_column - self.min_column + R::Item::one();
        let height = self.max_row - self.min_row + R::Item::one();

        Some(Rect::new(
            self.min_column,
            self.min_row,
            R::Item::create_non_zero(width).expect("width should be non-zero"),
            R::Item::create_non_zero(height).expect("height should be non-zero"),
        ))
    }
}

impl<T, R> Iterator for BoundsInspector<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: Bounded
        + Copy
        + Ord
        + std::ops::Rem<Output = R::Item>
        + std::ops::Div<Output = R::Item>
        + std::ops::Sub<Output = R::Item>
        + Zero
        + One,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.parent.next()?;

        let start = item.start();
        let end = item.end();
        let width_val: R::Item = self.width.into();

        let start_row = start / width_val;
        let start_col = start % width_val;

        let last = end - R::Item::one();
        let end_row = last / width_val;
        let end_col = last % width_val;

        self.min_row = self.min_row.min(start_row);
        self.max_row = self.max_row.max(end_row);

        if start_row == end_row {
            self.min_column = self.min_column.min(start_col);
            self.max_column = self.max_column.max(end_col);
        } else {
            self.min_column = R::Item::zero();
            self.max_column = width_val - R::Item::one();
        }

        Some(item)
    }
}

impl<T, R> FusedIterator for BoundsInspector<T, R>
where
    T: FusedIterator<Item = R>,
    R: CreateRange,
    R::Item: Bounded
        + Copy
        + Ord
        + std::ops::Rem<Output = R::Item>
        + std::ops::Div<Output = R::Item>
        + std::ops::Sub<Output = R::Item>
        + Zero
        + One,
{
}

impl<T, R> ImageDimension for BoundsInspector<T, R>
where
    T: Iterator<Item = R> + ImageDimension,
    R: CreateRange,
{
    fn width(&self) -> NonZero<u32> {
        self.parent.width()
    }
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_0_5_interop {
    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};

    use super::*;

    impl<T, TRangeItem> SortedStarts<TRangeItem> for BoundsInspector<T, RangeInclusive<TRangeItem>>
    where
        T: SortedStarts<TRangeItem>,
        TRangeItem: Integer
            + Bounded
            + Zero
            + One
            + SignedNonZeroable
            + std::ops::Add<Output = TRangeItem>
            + std::ops::Sub<Output = TRangeItem>
            + std::ops::Rem<Output = TRangeItem>
            + std::ops::Div<Output = TRangeItem>,
    {
    }

    impl<T, TRangeItem> SortedDisjoint<TRangeItem> for BoundsInspector<T, RangeInclusive<TRangeItem>>
    where
        T: SortedDisjoint<TRangeItem>,
        TRangeItem: Integer
            + Bounded
            + Zero
            + One
            + SignedNonZeroable
            + std::ops::Add<Output = TRangeItem>
            + std::ops::Sub<Output = TRangeItem>
            + std::ops::Rem<Output = TRangeItem>
            + std::ops::Div<Output = TRangeItem>,
    {
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZero, ops::Range};

    use super::*;
    use crate::{ImageDimension, WithBounds};

    const NON_ZERO_TEN: NonZero<usize> = NonZero::new(10).unwrap();
    const WIDTH_U32: NonZero<u32> = unsafe { NonZero::new_unchecked(10u32) };

    #[test]
    fn single_range_crossing_image_width() {
        let source = WithBounds::new([1..28usize].into_iter(), WIDTH_U32);
        let mut inspector = BoundsInspector::<_, Range<usize>>::new(source, NON_ZERO_TEN);
        assert_eq!(1, (&mut inspector).count());
        assert_eq!(
            inspector.bounds(),
            Some(Rect::new(
                0usize,
                0,
                NonZero::new(10).unwrap(),
                NonZero::new(3).unwrap()
            ))
        );
        assert_eq!(inspector.width(), WIDTH_U32);
    }

    #[test]
    fn multiple_ranges_with_different_lengths_and_row_gaps() {
        let source = [3..6usize, 30..33, 55..65];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let mut inspector = BoundsInspector::<_, Range<usize>>::new(source, NON_ZERO_TEN);
        let count = (&mut inspector).count();
        assert_eq!(count, 3);
        assert_eq!(
            inspector.bounds(),
            Some(Rect::new(
                0usize,
                0,
                NonZero::new(10).unwrap(),
                NonZero::new(7).unwrap()
            ))
        );
        assert_eq!(inspector.width(), WIDTH_U32);
    }

    #[test]
    fn empty_iterator_returns_none() {
        let source: [Range<usize>; 0] = [];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let inspector = BoundsInspector::<_, Range<usize>>::new(source, NON_ZERO_TEN);
        assert_eq!(inspector.bounds(), None);
        assert_eq!(inspector.width(), WIDTH_U32);
    }
}
