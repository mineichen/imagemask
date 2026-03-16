use std::{
    fmt::{Debug, Display},
    num::NonZero,
    ops::Range,
};

use crate::{CreateRange, NonZeroRange, SignedNonZeroable, UncheckedCast};

mod iter;
mod map_inplace;
mod offsets_iter;

pub use iter::*;
pub use map_inplace::*;
pub use offsets_iter::*;

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
}
impl<TIncluded, TExcluded> Debug for SortedRanges<TIncluded, TExcluded> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NonEmptyOrderedRanges")
            .field("range_count", &self.included.len())
            .finish()
    }
}

impl<TIncluded, TExcluded> SortedRanges<TIncluded, TExcluded> {
    pub fn new<TRange>(r: NonZeroRange<TRange>) -> Self
    where
        TRange:
            UncheckedCast<TIncluded> + UncheckedCast<TExcluded> + std::ops::Sub<Output = TRange>,
        TIncluded: TryFrom<u64>,
    {
        Self {
            included: vec![r.len().cast_unchecked()],
            excluded: vec![r.start.cast_unchecked()],
        }
    }

    pub fn try_from_ordered_iter<TRange>(
        iter: impl IntoIterator<Item = Range<TRange>>,
    ) -> Result<Self, String>
    where
        TRange: Into<u64>,
        TIncluded: TryFrom<u64, Error: Display>,
        TExcluded: TryFrom<u64, Error: Display>,
    {
        fn create_checked<T: TryFrom<u64, Error: Display>>(
            start: u64,
            end: u64,
        ) -> Result<T, String> {
            if end <= start {
                return Err(format!("{} must be > {}", end, start));
            }
            T::try_from(end - start).map_err(|e| e.to_string())
        }

        let mut iter = iter.into_iter().map(|range| {
            let start = range.start.into();
            let end = range.end.into();
            create_checked::<TIncluded>(start, end).map(|x| (start..end, x))
        });
        let Some((first_range, first_len)) = iter.next().transpose()? else {
            return Err("Requires at least one item".into());
        };
        let initial_offset = TExcluded::try_from(first_range.start).map_err(|e| e.to_string())?;
        let mut included = Vec::<TIncluded>::with_capacity(iter.size_hint().0 + 1);
        let mut excluded = Vec::<TExcluded>::with_capacity(iter.size_hint().0 + 1);

        included.push(first_len);
        excluded.push(initial_offset);
        let mut cur_pos = first_range.end;
        for x in iter {
            let (next_range, next_len) = x?;
            excluded.push(create_checked(cur_pos, next_range.start)?);
            included.push(next_len);
            cur_pos = next_range.end;
        }

        Ok(Self { included, excluded })
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
        T::Item: Default + Copy + SignedNonZeroable + std::ops::Add<Output = T::Item>,
    {
        SortedRangesIter::new(
            self.included.iter().copied(),
            self.excluded.iter().copied(),
            T::Item::default(),
        )
    }
    pub fn iter_owned<T: CreateRange>(
        self,
    ) -> SortedRangesIter<std::vec::IntoIter<TIncluded>, std::vec::IntoIter<TExcluded>, T>
    where
        TIncluded: UncheckedCast<T::Item>,
        TExcluded: UncheckedCast<T::Item>,
        T::Item: Default + Copy + SignedNonZeroable + std::ops::Add<Output = T::Item>,
    {
        SortedRangesIter::new(
            self.included.into_iter(),
            self.excluded.into_iter(),
            T::Item::default(),
        )
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
        SortedRangesIter::new(self.included.into_iter(), self.excluded.into_iter(), 0u64)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::RangeInclusive;

    use super::*;

    #[cfg(feature = "range-set-blaze-0_5")]
    #[test]
    fn combine_inline() {
        use range_set_blaze_0_5::SortedDisjoint;

        let a = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 30..40]).unwrap();
        let b = SortedRanges::<u8, u8>::try_from_ordered_iter([20u32..30, 41..45]).unwrap();

        let b_iter = b.iter::<RangeInclusive<u64>>();
        let a = a.map_inplace(|a_iter| b_iter.union(a_iter)).unwrap();

        assert_eq!(vec![10u64..40, 41..45], a.iter_owned().collect::<Vec<_>>());
        assert_eq!(vec![20u64..30, 41..45], b.iter_owned().collect::<Vec<_>>());
    }

    #[test]
    fn ranges_starting_at_zero() {
        let map = SortedRanges::<u32, u32>::try_from_ordered_iter([0u64..1, 5u64..6]);

        let map = map.unwrap();
        let collected: Vec<_> = map.iter::<std::ops::Range<u64>>().collect();
        assert_eq!(vec![0u64..1, 5u64..6], collected);
    }

    #[test]
    fn split_when_collection_becomes_bigger() {
        let a = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..15, 30..35]).unwrap();

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
        let a = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..15]).unwrap();

        let result = a.map_inplace(|_| std::iter::empty());

        assert!(result.is_none());
    }

    #[test]
    fn range_with_initial_offset() {
        let encoded = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257]).unwrap();
        assert_eq!(
            vec![10u64..=19, 255u64..=256],
            encoded.iter_owned().collect::<Vec<_>>()
        );
    }

    #[test]
    fn owned_iterator() {
        let encoded = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257]).unwrap();
        let collected: Vec<_> = encoded.iter_owned().collect();
        assert_eq!(2, collected.len());
        assert_eq!(10u64..=19, collected[0]);
        assert_eq!(255u64..=256, collected[1]);
    }
    #[test]
    fn owned_into_iterator() {
        let encoded = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257]).unwrap();
        let collected: Vec<_> = encoded.into_iter().collect();
        assert_eq!(2, collected.len());
        assert_eq!(NonZeroRange::new(10u64..20), collected[0]);
        assert_eq!(NonZeroRange::new(255u64..257), collected[1]);
    }

    #[test]
    fn assert_big_gap_causes_error() {
        let error =
            SortedRanges::<u16, u8>::try_from_ordered_iter([10u32..20, 276..280]).unwrap_err();
        assert!(error.contains("out of range"), "{error}");
    }

    #[test]
    fn assert_big_ranges_cause_error() {
        let error = SortedRanges::<u8, u16>::try_from_ordered_iter([10u32..280]).unwrap_err();
        assert!(error.contains("out of range"), "{error}");
    }
    #[test]
    fn zero_ranges_cause_error() {
        let error = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..10]).unwrap_err();
        assert!(error.contains("> 10"), "{error}");
    }

    #[test]
    fn overlapping_cause_error() {
        let error = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..12, 11..12]).unwrap_err();
        assert!(error.contains("> 12"), "{error}");
    }

    #[test]
    fn iterate_with_different_output_types() {
        let encoded = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..15, 30..35]).unwrap();

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
