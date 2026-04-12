use std::{
    fmt::{Debug, Display},
    io,
    num::NonZero,
    ops::{Add, Sub},
};

fn invalid_data<T: Display>(e: T) -> std::io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e.to_string())
}
use crate::{CreateRange, ImageDimension, NonZeroRange, Rect, SignedNonZeroable, UncheckedCast};

mod bounds_inspector;
mod chunk_by_row;
mod clip_2d;
#[cfg(feature = "async-io")]
mod future;
mod iter;
mod map_inplace;
mod offsets_iter;
mod rect;
mod sanitize_sorted_disjoint;
mod split_rows;

pub use bounds_inspector::*;
pub use chunk_by_row::*;
pub use clip_2d::*;
pub use iter::*;
pub use map_inplace::*;
pub use offsets_iter::*;
pub use rect::*;
pub use sanitize_sorted_disjoint::*;
pub use split_rows::*;

pub trait ImaskSet: Iterator + Sized + ImageDimension {
    /// # Panics
    /// If the previous RowIterator is kept when getting the next RowIterator
    fn chunk_by_row_lending<R: CreateRange<Item: SignedNonZeroable>>(
        self,
    ) -> ChunkByRowRanges<Self, R> {
        ChunkByRowRanges::new(self)
    }

    fn inspect_bounds<R>(self) -> BoundsInspector<Self, R>
    where
        R: CreateRange<Item: SignedNonZeroable>,
    {
        BoundsInspector::new(self)
    }

    fn try_clip_2d(
        self,
        roi: Rect<u32>,
    ) -> Result<Clip2dIter<Self, Self::Item>, RoiWidthExceedsOriginal> {
        Clip2dIter::try_new(self, roi)
    }

    fn split_rows(self) -> SplitRowsIter<Self, Self::Item>
    where
        Self::Item: CreateRange,
    {
        SplitRowsIter::new(self)
    }
}

impl<I: Iterator + ImageDimension> ImaskSet for I {}

/// Represents areas on images. It's designed to efficiently support various image sizes.
/// Both, TIncluded and TExcluded are expected to always be > 0. Use non-zero signed types
/// Included represents the number of pixels to include, excluded encodes the gap between two included ranges
///
/// Included.len() = excluded.len() + 1
///
/// Meta is expected to be indexable for each included range
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(feature = "rkyv", derive(rkyv::Archive))]
pub struct SortedRanges<TIncluded, TExcluded> {
    included: Vec<TIncluded>,
    excluded: Vec<TExcluded>,
    bounds: Rect<u32>,
}
impl<TIncluded, TExcluded> Debug for SortedRanges<TIncluded, TExcluded> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NonEmptyOrderedRanges")
            .field("range_count", &self.included.len())
            .finish()
    }
}
struct Builder<TIncluded, TExcluded> {
    cur_pos: u64,
    included: Vec<TIncluded>,
    excluded: Vec<TExcluded>,
}

impl<TIncluded, TExcluded> Builder<TIncluded, TExcluded>
where
    TIncluded: TryFrom<u64, Error: Display>,
    TExcluded: TryFrom<u64, Error: Display>,
{
    fn new<TRange>(first_range: TRange, size_hint: usize) -> Result<Self, io::Error>
    where
        TRange: CreateRange<Item: TryInto<u64, Error: Display>>,
    {
        let (start_u64, end_u64) = (
            first_range.start().try_into().map_err(invalid_data)?,
            first_range.end().try_into().map_err(invalid_data)?,
        );
        let first_len = create_checked(start_u64, end_u64)?;
        let initial_offset = TExcluded::try_from(start_u64).map_err(invalid_data)?;
        let mut included = Vec::<TIncluded>::with_capacity(size_hint);
        let mut excluded = Vec::<TExcluded>::with_capacity(size_hint);
        included.push(first_len);
        excluded.push(initial_offset);
        Ok(Self {
            included,
            excluded,
            cur_pos: end_u64,
        })
    }

    fn add<TRange>(&mut self, range: TRange) -> Result<(), io::Error>
    where
        TRange: CreateRange<Item: TryInto<u64, Error: Display>>,
    {
        let (start_u64, end_u64) = (
            range.start().try_into().map_err(invalid_data)?,
            range.end().try_into().map_err(invalid_data)?,
        );
        self.excluded.push(create_checked(self.cur_pos, start_u64)?);
        self.included.push(create_checked(start_u64, end_u64)?);
        self.cur_pos = end_u64;
        Ok(())
    }
    fn build(self, bounds: Rect<u32>) -> SortedRanges<TIncluded, TExcluded> {
        SortedRanges {
            included: self.included,
            excluded: self.excluded,
            bounds,
        }
    }
}
fn create_checked<T>(start: u64, end: u64) -> Result<T, io::Error>
where
    T: TryFrom<u64, Error: Display>,
{
    if end <= start {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("end ({end}) must be > start ({start})"),
        ));
    }
    T::try_from(end - start).map_err(invalid_data)
}

impl<TIncluded, TExcluded> SortedRanges<TIncluded, TExcluded> {
    pub fn new<TRange>(r: NonZeroRange<TRange>, bounds: Rect<u32>) -> Self
    where
        TRange: UncheckedCast<TIncluded> + UncheckedCast<TExcluded> + Sub<Output = TRange>,
        TIncluded: TryFrom<u64>,
    {
        assert!(bounds.x == 0);
        assert!(bounds.y == 0);
        Self {
            included: vec![r.len().cast_unchecked()],
            excluded: vec![r.start.cast_unchecked()],
            bounds,
        }
    }

    pub fn try_from_ordered_iter<TIter>(iter: TIter, bounds: Rect<u32>) -> Result<Self, io::Error>
    where
        TIter: IntoIterator<Item: CreateRange<Item: TryInto<u64, Error: Display>>>,
        TIncluded: TryFrom<u64, Error: Display>,
        TExcluded: TryFrom<u64, Error: Display>,
    {
        assert!(bounds.x == 0);
        assert!(bounds.y == 0);
        let mut iter = iter.into_iter();
        let Some(first_range) = iter.next() else {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Requires at least one item",
            ));
        };
        let mut builder = Builder::new(first_range, iter.size_hint().0 + 1)?;

        for x in iter {
            builder.add(x)?;
        }

        Ok(builder.build(bounds))
    }

    #[allow(clippy::len_without_is_empty, reason = "Cannot be empty")]
    pub fn len(&self) -> usize {
        self.included.len()
    }

    pub fn len_nonzero(&self) -> NonZero<usize> {
        NonZero::new(self.included.len())
            .expect("Constructors make sure, there is always at least one Range")
    }

    pub fn iter<T: CreateRange>(
        &self,
    ) -> SortedRangesIter<
        std::iter::Copied<std::slice::Iter<'_, TIncluded>>,
        std::iter::Copied<std::slice::Iter<'_, TExcluded>>,
        T,
    >
    where
        TIncluded: UncheckedCast<T::Item>,
        TExcluded: UncheckedCast<T::Item>,
        T::Item: Default + Copy + SignedNonZeroable + Add<Output = T::Item>,
    {
        SortedRangesIter::new(
            self.included.iter().copied(),
            self.excluded.iter().copied(),
            T::Item::default(),
            self.bounds,
        )
    }
    pub fn iter_owned<T: CreateRange>(
        self,
    ) -> SortedRangesIter<std::vec::IntoIter<TIncluded>, std::vec::IntoIter<TExcluded>, T>
    where
        TIncluded: UncheckedCast<T::Item>,
        TExcluded: UncheckedCast<T::Item>,
        T::Item: Default + Copy + SignedNonZeroable + Add<Output = T::Item>,
    {
        SortedRangesIter::new(
            self.included.into_iter(),
            self.excluded.into_iter(),
            T::Item::default(),
            self.bounds,
        )
    }
}

impl<TIncluded, TExcluded> ImageDimension for SortedRanges<TIncluded, TExcluded> {
    fn width(&self) -> NonZero<u32> {
        self.bounds.width
    }
}

impl<TIncluded: UncheckedCast<u64>, TExcluded: UncheckedCast<u64>> IntoIterator
    for SortedRanges<TIncluded, TExcluded>
{
    type Item = NonZeroRange<u64>;
    type IntoIter = SortedRangesIter<
        std::vec::IntoIter<TIncluded>,
        std::vec::IntoIter<TExcluded>,
        NonZeroRange<u64>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        SortedRangesIter::new(
            self.included.into_iter(),
            self.excluded.into_iter(),
            0u64,
            self.bounds,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::ops::{Range, RangeInclusive};

    use super::*;

    const TEST_BOUNDS: Rect<u32> = Rect::new(
        0,
        0,
        NonZero::new(1000u32).unwrap(),
        NonZero::new(1000u32).unwrap(),
    );

    #[cfg(feature = "range-set-blaze-0_5")]
    #[test]
    fn combine_inline() {
        use range_set_blaze_0_5::SortedDisjoint;

        let a = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 30..40], TEST_BOUNDS)
            .unwrap();
        let b = SortedRanges::<u8, u8>::try_from_ordered_iter([20u32..30, 41..45], TEST_BOUNDS)
            .unwrap();

        let b_iter = b.iter::<RangeInclusive<u64>>();
        let a = a.map_inplace(|a_iter| b_iter.union(a_iter)).unwrap();

        assert_eq!(vec![10u64..40, 41..45], a.iter_owned().collect::<Vec<_>>());
        assert_eq!(vec![20u64..30, 41..45], b.iter_owned().collect::<Vec<_>>());
    }

    #[test]
    fn ranges_starting_at_zero() {
        let map = SortedRanges::<u32, u32>::try_from_ordered_iter([0u64..1, 5u64..6], TEST_BOUNDS);

        let map = map.unwrap();
        let collected: Vec<_> = map.iter::<std::ops::Range<u64>>().collect();
        assert_eq!(vec![0u64..1, 5u64..6], collected);
    }

    #[test]
    fn split_when_collection_becomes_bigger() {
        let a = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..15, 30..35], TEST_BOUNDS)
            .unwrap();

        let a = a
            .map_inplace(|iter| {
                iter.flat_map(|x| {
                    let with_offset = (*x.start() + 10)..=(*x.end() + 10);
                    [x, with_offset]
                })
            })
            .unwrap();

        assert_eq!(
            vec![10u64..15, 20..25, 30..35, 40..45],
            a.iter_owned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn split_returns_none_when_empty() {
        let a = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..15], TEST_BOUNDS).unwrap();

        let result = a.map_inplace(|_| std::iter::empty());

        assert!(result.is_none());
    }

    #[test]
    fn range_with_initial_offset() {
        let encoded =
            SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257], TEST_BOUNDS)
                .unwrap();
        assert_eq!(
            vec![10u64..=19, 255u64..=256],
            encoded.iter_owned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn owned_iterator() {
        let encoded =
            SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257], TEST_BOUNDS)
                .unwrap();
        let collected: Vec<_> = encoded.iter_owned().collect();
        assert_eq!(2, collected.len());
        assert_eq!(10u64..=19, collected[0]);
        assert_eq!(255u64..=256, collected[1]);
    }
    #[test]
    fn owned_into_iterator() {
        let encoded =
            SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257], TEST_BOUNDS)
                .unwrap();
        let collected: Vec<_> = encoded.into_iter().collect();
        assert_eq!(2, collected.len());
        assert_eq!(NonZeroRange::new(10u64..20), collected[0]);
        assert_eq!(NonZeroRange::new(255u64..257), collected[1]);
    }

    #[test]
    fn assert_big_gap_causes_error() {
        let error =
            SortedRanges::<u16, u8>::try_from_ordered_iter([10u32..20, 276..280], TEST_BOUNDS)
                .unwrap_err();
        assert!(error.to_string().contains("out of range"), "{error}");
    }

    #[test]
    fn assert_big_ranges_cause_error() {
        let error =
            SortedRanges::<u8, u16>::try_from_ordered_iter([10u32..280], TEST_BOUNDS).unwrap_err();
        assert!(error.to_string().contains("out of range"), "{error}");
    }
    #[test]
    fn zero_ranges_cause_error() {
        let error =
            SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..10], TEST_BOUNDS).unwrap_err();
        assert!(error.to_string().contains("must be >"), "{error}");
    }

    #[test]
    fn overlapping_cause_error() {
        let error = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..12, 11..12], TEST_BOUNDS)
            .unwrap_err();
        assert!(error.to_string().contains("must be >"), "{error}");
    }

    #[test]
    fn iterate_with_different_output_types() {
        let encoded =
            SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..15, 30..35], TEST_BOUNDS)
                .unwrap();

        let as_range: Vec<_> = encoded.iter::<Range<u64>>().collect();
        assert_eq!(vec![10u64..15, 30..35], as_range);

        let as_range_inclusive: Vec<_> = encoded.iter::<RangeInclusive<u64>>().collect();
        assert_eq!(vec![10u64..=14, 30..=34], as_range_inclusive);

        let as_nonzero_range: Vec<_> = encoded.iter::<NonZeroRange<u64>>().collect();
        assert_eq!(
            vec![NonZeroRange::new(10u64..15), NonZeroRange::new(30..35)],
            as_nonzero_range
        );
    }
}
