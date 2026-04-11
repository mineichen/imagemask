use std::{fmt::Debug, iter::FusedIterator, marker::PhantomData, num::NonZero};

use crate::{CreateRange, ImageDimension, SignedNonZeroable};

pub struct SplitRowsIter<T: Iterator, R: CreateRange> {
    parent: T,
    width: <R::Item as SignedNonZeroable>::NonZero,
    pending: Option<R>,
    _range: PhantomData<R>,
}

impl<T: Iterator, R: CreateRange> SplitRowsIter<T, R> {
    pub fn new(parent: T, width: <R::Item as SignedNonZeroable>::NonZero) -> Self {
        Self {
            parent,
            width,
            pending: None,
            _range: PhantomData,
        }
    }
}

impl<T: Iterator, R: CreateRange<Item: Debug>> Debug for SplitRowsIter<T, R>
where
    <R::Item as SignedNonZeroable>::NonZero: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplitRowsIter")
            .field("width", &self.width)
            .finish()
    }
}

impl<T, R> Iterator for SplitRowsIter<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange<
        Item: Copy
                  + Ord
                  + std::ops::Sub<Output = R::Item>
                  + std::ops::Add<Output = R::Item>
                  + std::ops::Mul<Output = R::Item>
                  + std::ops::Div<Output = R::Item>,
    >,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        let width: R::Item = self.width.into();

        let range = self.pending.take().or_else(|| self.parent.next())?;

        let start = range.start();
        let end = range.end();

        let row_start = start / width * width;
        let next_row_start = row_start + width;

        if end <= next_row_start {
            Some(range)
        } else {
            let clip_len = SignedNonZeroable::create_non_zero(next_row_start - start)
                .expect("Mustn't be zero");
            let remaining_len =
                unsafe { SignedNonZeroable::create_non_zero_unchecked(end - next_row_start) };

            self.pending = Some(R::new_debug_checked(next_row_start, remaining_len));
            Some(R::new_debug_checked(start, clip_len))
        }
    }
}

impl<T, R> FusedIterator for SplitRowsIter<T, R>
where
    T: FusedIterator<Item = R>,
    R: CreateRange<
        Item: Copy
                  + Ord
                  + std::ops::Sub<Output = R::Item>
                  + std::ops::Add<Output = R::Item>
                  + std::ops::Mul<Output = R::Item>
                  + std::ops::Div<Output = R::Item>,
    >,
{
}

impl<T, R> ImageDimension for SplitRowsIter<T, R>
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
    use range_set_blaze_0_5::{Integer, SortedStarts};
    use std::ops::RangeInclusive;

    use super::*;

    impl<T, TRangeItem> SortedStarts<TRangeItem> for SplitRowsIter<T, RangeInclusive<TRangeItem>>
    where
        T: SortedStarts<TRangeItem>,
        TRangeItem: Integer
            + Copy
            + Ord
            + SignedNonZeroable
            + num_traits::One
            + std::ops::Add<Output = TRangeItem>
            + std::ops::Sub<Output = TRangeItem>
            + std::ops::Mul<Output = TRangeItem>
            + std::ops::Div<Output = TRangeItem>,
    {
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZero, ops::Range};

    use super::*;
    use crate::{ImageDimension, WithBounds};

    const WIDTH: NonZero<usize> = NonZero::new(10).unwrap();
    const WIDTH_U32: NonZero<u32> = unsafe { NonZero::new_unchecked(10u32) };

    #[test]
    fn range_within_single_row() {
        let source = WithBounds::new([0..5usize].into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source, WIDTH).collect();
        assert_eq!(result, vec![0..5]);
    }

    #[test]
    fn range_crossing_one_row_boundary() {
        let source = WithBounds::new([5..15usize].into_iter(), WIDTH_U32);
        let split = SplitRowsIter::new(source, WIDTH);
        assert_eq!(split.width(), WIDTH_U32);
        let result: Vec<_> = split.collect();
        assert_eq!(result, vec![5..10, 10..15]);
    }

    #[test]
    fn range_spanning_three_rows() {
        let source = [0..25usize];
        let result: Vec<_> = SplitRowsIter::new(source.into_iter(), WIDTH).collect();
        assert_eq!(result, vec![0..10, 10..20, 20..25]);
    }

    #[test]
    fn range_exactly_one_row() {
        let source = [10..20usize];
        let result: Vec<_> = SplitRowsIter::new(source.into_iter(), WIDTH).collect();
        assert_eq!(result, vec![10..20]);
    }

    #[test]
    fn multiple_ranges_some_crossing() {
        let source = [0..3usize, 6..12, 15..20];
        let result: Vec<_> = SplitRowsIter::new(source.into_iter(), WIDTH).collect();
        assert_eq!(result, vec![0..3, 6..10, 10..12, 15..20]);
    }

    #[test]
    fn empty_iterator() {
        let result: Vec<Range<usize>> = SplitRowsIter::new(std::iter::empty(), WIDTH).collect();
        assert!(result.is_empty());
    }

    #[test]
    fn single_pixel() {
        let source = [5..6usize];
        let result: Vec<_> = SplitRowsIter::new(source.into_iter(), WIDTH).collect();
        assert_eq!(result, vec![5..6]);
    }

    #[test]
    fn single_pixel_at_row_boundary() {
        let source = [10..11usize];
        let result: Vec<_> = SplitRowsIter::new(source.into_iter(), WIDTH).collect();
        assert_eq!(result, vec![10..11]);
    }

    #[test]
    fn range_starting_at_boundary_crossing_two_rows() {
        let source = [10..25usize];
        let result: Vec<_> = SplitRowsIter::new(source.into_iter(), WIDTH).collect();
        assert_eq!(result, vec![10..20, 20..25]);
    }

    #[test]
    fn range_spanning_many_rows() {
        let source = [3..97usize];
        let result: Vec<_> = SplitRowsIter::new(source.into_iter(), WIDTH).collect();
        assert_eq!(
            result,
            vec![
                3..10,
                10..20,
                20..30,
                30..40,
                40..50,
                50..60,
                60..70,
                70..80,
                80..90,
                90..97
            ]
        );
    }
}
