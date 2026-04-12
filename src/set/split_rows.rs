use std::{fmt::Debug, iter::FusedIterator, marker::PhantomData, num::NonZero};

use crate::{CreateRange, ImageDimension, SignedNonZeroable, UncheckedCast};

pub struct SplitRowsIter<T, R> {
    parent: T,
    pending: Option<R>,
    _range: PhantomData<R>,
}

impl<T, R> SplitRowsIter<T, R> {
    pub fn new(parent: T) -> Self {
        Self {
            parent,
            pending: None,
            _range: PhantomData,
        }
    }
}

impl<T: ImageDimension, R> Debug for SplitRowsIter<T, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SplitRowsIter")
            .field("width", &self.parent.width())
            .finish()
    }
}

impl<T, R> Iterator for SplitRowsIter<T, R>
where
    T: Iterator<Item = R> + ImageDimension,
    R: CreateRange<
        Item: Copy
                  + Ord
                  + std::ops::Sub<Output = R::Item>
                  + std::ops::Add<Output = R::Item>
                  + std::ops::Mul<Output = R::Item>
                  + std::ops::Div<Output = R::Item>,
    >,
    u32: UncheckedCast<R::Item>,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        let width: R::Item = self.width().get().cast_unchecked();

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
    SplitRowsIter<T, R>: Iterator,
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
        TRangeItem: Integer,
        T: SortedStarts<TRangeItem>,
        SplitRowsIter<T, RangeInclusive<TRangeItem>>:
            FusedIterator<Item = RangeInclusive<TRangeItem>>,
    {
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZero, ops::Range};

    use super::*;
    use crate::{ImageDimension, WithBounds};

    const WIDTH_U32: NonZero<u32> = unsafe { NonZero::new_unchecked(10u32) };

    #[test]
    fn range_within_single_row() {
        let source = WithBounds::new([0..5usize].into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source).collect();
        assert_eq!(result, vec![0..5]);
    }

    #[test]
    fn range_crossing_one_row_boundary() {
        let source = WithBounds::new([5..15usize].into_iter(), WIDTH_U32);
        let split = SplitRowsIter::new(source);
        assert_eq!(split.width(), WIDTH_U32);
        let result: Vec<_> = split.collect();
        assert_eq!(result, vec![5..10, 10..15]);
    }

    #[test]
    fn range_spanning_three_rows() {
        let source = [0..25usize];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source.into_iter()).collect();
        assert_eq!(result, vec![0..10, 10..20, 20..25]);
    }

    #[test]
    fn range_exactly_one_row() {
        let source = [10..20usize];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source).collect();
        assert_eq!(result, vec![10..20]);
    }

    #[test]
    fn multiple_ranges_some_crossing() {
        let source = [0..3usize, 6..12, 15..20];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source).collect();
        assert_eq!(result, vec![0..3, 6..10, 10..12, 15..20]);
    }

    #[test]
    fn empty_iterator() {
        let source = WithBounds::new(std::iter::empty(), WIDTH_U32);
        let result: Vec<Range<usize>> = SplitRowsIter::new(source).collect();
        assert!(result.is_empty());
    }

    #[test]
    fn single_pixel() {
        let source = [5..6usize];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source).collect();
        assert_eq!(result, vec![5..6]);
    }

    #[test]
    fn single_pixel_at_row_boundary() {
        let source = [10..11usize];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source).collect();
        assert_eq!(result, vec![10..11]);
    }

    #[test]
    fn range_starting_at_boundary_crossing_two_rows() {
        let source = [10..25usize];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source).collect();
        assert_eq!(result, vec![10..20, 20..25]);
    }

    #[test]
    fn range_spanning_many_rows() {
        let source = [3..97usize];
        let source = WithBounds::new(source.into_iter(), WIDTH_U32);
        let result: Vec<_> = SplitRowsIter::new(source).collect();
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
