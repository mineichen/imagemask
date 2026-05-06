use std::{fmt::Debug, iter::FusedIterator, num::NonZero};

use num_traits::{One, Zero};

use crate::{CreateRange, ImageDimension, Rect, SignedNonZeroable, UncheckedCast};

pub struct Clip2dIter<T, R> {
    parent: T,
    roi: Rect<u32>,
    pending: Option<R>,
    pending_source: Option<(R, u32)>,
}

impl<T: Iterator + ImageDimension, R: CreateRange<Item: Debug>> Debug for Clip2dIter<T, R>
where
    <R::Item as SignedNonZeroable>::NonZero: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clip2dIter")
            .field("sub", &self.roi)
            .field("outer_width", &self.width())
            .finish()
    }
}

impl<T, R> Clip2dIter<T, R>
where
    T: ImageDimension,
{
    pub fn new(parent: T, roi: Rect<u32>) -> Self {
        Self {
            parent,
            roi,
            pending: None,
            pending_source: None,
        }
    }
}

impl<T, R> Iterator for Clip2dIter<T, R>
where
    T: Iterator<Item = R> + ImageDimension,
    R: CreateRange<
        Item: Copy
                  + Ord
                  + Zero
                  + One
                  + std::ops::Sub<Output = R::Item>
                  + std::ops::Add<Output = R::Item>
                  + std::ops::Mul<Output = R::Item>
                  + std::ops::Div<Output = R::Item>
                  + std::ops::Rem<Output = R::Item>
                  + UncheckedCast<u32>,
    >,
    u32: UncheckedCast<R::Item>,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        let outer_w: R::Item = self.parent.width().get().cast_unchecked();
        let sub_w: R::Item = self.roi.width.get().cast_unchecked();
        let sub_x = self.roi.x.cast_unchecked();
        let sub_y = self.roi.y.cast_unchecked();
        let sub_h: R::Item = self.roi.height.get().cast_unchecked();

        let sub_row_start = sub_y;
        let sub_row_end = sub_y + sub_h;
        let sub_col_start = sub_x;
        let sub_col_end_raw = sub_x + sub_w;
        let sub_col_end = sub_col_end_raw.min(outer_w);
        let roi_exceeds = sub_col_end < sub_col_end_raw;

        loop {
            if let Some((item, row_u32)) = self.pending_source.take() {
                let row: R::Item = row_u32.cast_unchecked();

                let item_start = item.start();
                let item_end = item.end();
                let last_row_of_item = (item_end - R::Item::one()) / outer_w;
                let first_row_of_item = item_start / outer_w;

                let row_col_start = if row == first_row_of_item {
                    item_start % outer_w
                } else {
                    R::Item::zero()
                };
                let row_col_end = if row == last_row_of_item {
                    (item_end - R::Item::one()) % outer_w + R::Item::one()
                } else {
                    outer_w
                };

                if row < last_row_of_item {
                    self.pending_source = Some((item, row_u32 + 1));
                }

                if row < sub_row_start || row >= sub_row_end {
                    continue;
                }

                let c_first = row_col_start.max(sub_col_start);
                let c_last = row_col_end.min(sub_col_end);
                if c_last <= c_first {
                    continue;
                }

                let sub_row = row - sub_y;
                let sub_first_col = c_first - sub_x;
                let sub_last_col = c_last - sub_x;
                let sub_start = sub_row * sub_w + sub_first_col;
                let sub_end = sub_row * sub_w + sub_last_col;

                match self.pending.take() {
                    Some(x) => {
                        if x.end() == sub_start {
                            self.pending = Some(R::new_debug_checked(
                                x.start(),
                                R::Item::create_non_zero(sub_end - x.start()).unwrap(),
                            ));
                        } else {
                            self.pending = Some(R::new_debug_checked(
                                sub_start,
                                R::Item::create_non_zero(sub_end - sub_start).unwrap(),
                            ));
                            return Some(x);
                        }
                    }
                    None => {
                        self.pending = Some(R::new_debug_checked(
                            sub_start,
                            R::Item::create_non_zero(sub_end - sub_start).unwrap(),
                        ));
                    }
                };
                continue;
            }

            let Some(item) = self.parent.next() else {
                return self.pending.take();
            };
            let start = item.start();
            let end = item.end();

            let first_row = start / outer_w;
            let last_row = (end - R::Item::one()) / outer_w;
            let first_col = start % outer_w;
            let last_col = (end - R::Item::one()) % outer_w;

            if first_row >= sub_row_end {
                return self.pending.take();
            }
            if last_row < sub_row_start {
                continue;
            }

            let clipped_first_row = first_row.max(sub_row_start);
            let clipped_last_row = last_row.min(sub_row_end - R::Item::one());

            if roi_exceeds && clipped_first_row != clipped_last_row {
                let first_row_u32: u32 = (start / outer_w).cast_unchecked();
                let clipped_first_row_u32 = first_row_u32.max(self.roi.y);
                self.pending_source = Some((item, clipped_first_row_u32));
                continue;
            }

            let clipped_first_col = if clipped_first_row == first_row {
                first_col.max(sub_col_start)
            } else {
                sub_col_start
            };

            let clipped_last_col = if clipped_last_row == last_row {
                last_col.min(sub_col_end - R::Item::one())
            } else {
                sub_col_end - R::Item::one()
            };

            if clipped_first_col >= sub_col_end || clipped_last_col < sub_col_start {
                continue;
            }

            let sub_first_row = clipped_first_row - sub_y;
            let sub_last_row = clipped_last_row - sub_y;
            let sub_first_col = clipped_first_col - sub_x;
            let sub_last_col = clipped_last_col - sub_x;

            let sub_start = sub_first_row * sub_w + sub_first_col;
            let sub_end = sub_last_row * sub_w + sub_last_col + R::Item::one();

            debug_assert!(sub_start < sub_end, "Input must be SortedDisjoint");

            match self.pending.take() {
                Some(x) => {
                    if x.end() == sub_start {
                        self.pending = Some(R::new_debug_checked(
                            x.start(),
                            R::Item::create_non_zero(sub_end - x.start()).unwrap(),
                        ));
                    } else {
                        self.pending = Some(R::new_debug_checked(
                            sub_start,
                            R::Item::create_non_zero(sub_end - sub_start).unwrap(),
                        ));
                        return Some(x);
                    }
                }
                None => {
                    self.pending = Some(R::new_debug_checked(
                        sub_start,
                        R::Item::create_non_zero(sub_end - sub_start).unwrap(),
                    ));
                }
            };
        }
    }
}

impl<T, R> FusedIterator for Clip2dIter<T, R>
where
    T: FusedIterator<Item = R>,
    Clip2dIter<T, R>: Iterator,
{
}

impl<T, R> ImageDimension for Clip2dIter<T, R>
where
    T: Iterator + ImageDimension,
    R: CreateRange,
{
    fn width(&self) -> NonZero<u32> {
        self.roi.width
    }

    fn bounds(&self) -> Rect<u32> {
        self.roi
    }
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_impl {
    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};
    use std::ops::RangeInclusive;

    use super::*;

    impl<T, TRangeItem> SortedStarts<TRangeItem> for Clip2dIter<T, RangeInclusive<TRangeItem>>
    where
        TRangeItem: Integer,
        RangeInclusive<TRangeItem>: CreateRange,
        Clip2dIter<T, RangeInclusive<TRangeItem>>: FusedIterator<Item = RangeInclusive<TRangeItem>>,
        // where
        //     T: SortedStarts<TRangeItem>,
        //     TRangeItem: Integer
        //         + Copy
        //         + Ord
        //         + Zero
        //         + One
        //         + SignedNonZeroable
        //         + std::ops::Add<Output = TRangeItem>
        //         + std::ops::Sub<Output = TRangeItem>
        //         + std::ops::Mul<Output = TRangeItem>
        //         + std::ops::Div<Output = TRangeItem>
        //         + std::ops::Rem<Output = TRangeItem>,
        //     u32: UncheckedCast<TRangeItem>,
    {
    }

    impl<T, TRangeItem> SortedDisjoint<TRangeItem> for Clip2dIter<T, RangeInclusive<TRangeItem>>
    where
        TRangeItem: Integer,
        RangeInclusive<TRangeItem>: CreateRange,
        Clip2dIter<T, RangeInclusive<TRangeItem>>: FusedIterator<Item = RangeInclusive<TRangeItem>>,
    {
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZero, ops::Range};

    use crate::{ImageDimension, ImaskSet};

    use super::*;

    const WIDTH_U32: NonZero<u32> = NonZero::new(10u32).unwrap();

    #[test]
    fn clip_full_rect_produces_single_range() {
        const RECT_SIZE: NonZero<u32> = NonZero::new(5).unwrap();
        let rect = Rect::new(2u32, 2, RECT_SIZE, RECT_SIZE);

        let ranges = rect
            .into_rect_iter::<Range<u32>>(WIDTH_U32)
            .clip(rect)
            .collect::<Vec<_>>();
        assert_eq!(vec![0..25], ranges);
    }

    #[test]
    fn range_crossing_row_boundary_but_exceets_roi_height() {
        let sub = Rect::new(3, 1, NonZero::new(4).unwrap(), NonZero::new(2).unwrap());
        let source = [12..25usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(sub).collect();
        assert_eq!(result, vec![0..6,]);
    }

    #[test]
    fn adjacent_across_row_boundary() {
        let sub = Rect::new(0, 0, NonZero::new(10).unwrap(), NonZero::new(2).unwrap());
        let source = [5..25usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(sub).collect();
        assert_eq!(result, vec![5..20]);
    }

    #[test]
    fn range_entirely_outside_is_skipped() {
        let sub = Rect::new(3, 1, NonZero::new(4).unwrap(), NonZero::new(2).unwrap());
        let source = [0..3usize].with_bounds(WIDTH_U32, WIDTH_U32);
        assert_eq!(source.clip(sub).count(), 0);
    }

    #[test]
    fn range_clipped_at_right_edge() {
        let new_width = NonZero::new(4).unwrap();
        let sub = Rect::new(3, 1, new_width, NonZero::new(2).unwrap());
        let source = [12..18usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let clipped = source.clip(sub);
        assert_eq!(clipped.width(), new_width);
        let result: Vec<_> = clipped.collect();
        assert_eq!(result, vec![0..4]);
    }

    #[test]
    fn single_pixel_range() {
        let sub = Rect::new(3, 1, NonZero::new(4).unwrap(), NonZero::new(2).unwrap());
        let source = [24..25usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(sub).collect();
        assert_eq!(result, vec![5..6]);
    }

    #[test]
    fn clip_full_width() {
        let sub = Rect::new(0, 1, WIDTH_U32, NonZero::new(2).unwrap());
        let source = [24..25usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(sub).collect();
        assert_eq!(result, vec![14..15usize]);
    }

    #[test]
    fn clip_succeeds_when_roi_fits() {
        let roi = Rect::new(3, 1, NonZero::new(4).unwrap(), NonZero::new(2).unwrap());
        let source = [12..18usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..4]);
    }

    #[test]
    fn clip_when_roi_exceeds_width_produces_no_ranges_beyond_image() {
        let roi = Rect::new(8u32, 0, NonZero::new(5).unwrap(), NonZero::new(1).unwrap());
        let source = [0..10usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..2]);
    }

    #[test]
    fn roi_exceeding_image_width_clips_to_available_columns_full_width() {
        let orig_width = NonZero::new(10u32).unwrap();
        let source = [0u32..20].with_bounds(orig_width, orig_width);
        let result: Vec<_> = source
            .clip(Rect::new(
                8u32,
                0,
                NonZero::new(5).unwrap(),
                NonZero::new(2).unwrap(),
            ))
            .collect();
        assert_eq!(result, vec![0..2, 5..7]);
    }
    #[test]
    fn roi_exceeding_image_width_clips_to_available_columns() {
        let orig_width = NonZero::new(10u32).unwrap();
        let source = [8..10usize, 18..20].with_bounds(orig_width, orig_width);
        let result: Vec<_> = source
            .clip(Rect::new(
                8u32,
                0,
                NonZero::new(5).unwrap(),
                NonZero::new(2).unwrap(),
            ))
            .collect();
        assert_eq!(result, vec![0..2, 5..7]);
    }

    #[test]
    fn multiple_disjoint_ranges() {
        let roi = Rect::new(2, 0, NonZero::new(3).unwrap(), NonZero::new(2).unwrap());
        let source = [2..5usize, 12..15, 22..25].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..6]);
    }

    #[test]
    fn range_spanning_multiple_rows() {
        let roi = Rect::new(0, 1, NonZero::new(10).unwrap(), NonZero::new(3).unwrap());
        let source = [5..35usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..25]);
    }

    #[test]
    fn range_partially_inside_roi_left() {
        let roi = Rect::new(5, 0, NonZero::new(5).unwrap(), NonZero::new(1).unwrap());
        let source = [2..8usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..3]);
    }

    #[test]
    fn range_partially_inside_roi_right() {
        let roi = Rect::new(2, 0, NonZero::new(5).unwrap(), NonZero::new(1).unwrap());
        let source = [5..12usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![3..5]);
    }

    #[test]
    fn empty_iterator() {
        let roi = Rect::new(0, 0, WIDTH_U32, NonZero::new(1).unwrap());
        let source = std::iter::empty::<Range<usize>>().with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert!(result.is_empty());
    }

    #[test]
    fn roi_at_origin() {
        let roi = Rect::new(0, 0, NonZero::new(5).unwrap(), NonZero::new(2).unwrap());
        let source = [0..20usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..10]);
    }

    #[test]
    fn roi_exactly_image_bounds() {
        let roi = Rect::new(0, 0, WIDTH_U32, NonZero::new(3).unwrap());
        let source = [0..30usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..30]);
    }

    #[test]
    fn ranges_before_and_after_roi() {
        let roi = Rect::new(0, 1, WIDTH_U32, NonZero::new(1).unwrap());
        let source = [0..5usize, 20..25].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert!(result.is_empty());
    }

    #[test]
    fn single_pixel_at_roi_corner() {
        let roi = Rect::new(5, 2, NonZero::new(3).unwrap(), NonZero::new(2).unwrap());
        let source = [27..28usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![2..3]);
    }

    #[test]
    fn range_clipped_to_single_pixel() {
        let roi = Rect::new(9, 0, NonZero::new(1).unwrap(), NonZero::new(1).unwrap());
        let source = [8..12usize].with_bounds(WIDTH_U32, WIDTH_U32);
        let result: Vec<_> = source.clip(roi).collect();
        assert_eq!(result, vec![0..1]);
    }

    // Not yet supported... Might not be worth because of the performance penalty for storing pending items
    // This could be implemented zero-cost, if width is a interna of the iterators (we'd just decrement width until x+new_width = old_width)
    // #[test]
    // fn clip_more_than_full_width_adds_gaps_between_ranges_which_cross_line_ends() {
    //     let sub = Rect::new(1usize, 1, OUTER_W, NonZero::new(2).unwrap());
    //     let source = [24..35usize];
    //     let result: Vec<_> =
    //         SubImageIter::<_, Range<usize>>::new(source.into_iter(), sub, OUTER_W).collect();
    //     assert_eq!(result, vec![13..20usize, 21..24]);
    // }
}
