use std::{
    fmt::{Debug, Display},
    iter::FusedIterator,
    ops::{Range, RangeInclusive},
};

use range_set_blaze::{SortedDisjoint, SortedDisjointMap, SortedStarts, SortedStartsMap, ValueRef};

use crate::NonZeroRange;

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
    initial_offset: u64,
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
        TRange: Into<u64> + Into<TIncluded> + Copy + std::ops::Sub<Output = TRange>,
    {
        let len = r.len().into();
        Self {
            initial_offset: r.start.into(),
            included: vec![len],
            excluded: Vec::new(),
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
        let mut included = Vec::<TIncluded>::with_capacity(iter.size_hint().0);
        let mut excluded = Vec::<TExcluded>::with_capacity(iter.size_hint().0);

        included.push(first_len);
        let mut cur_pos = first_range.end;
        for x in iter {
            let (next_range, next_len) = x?;
            excluded.push(create_checked(cur_pos, next_range.start)?);
            included.push(next_len);
            cur_pos = next_range.end;
        }

        Ok(Self {
            initial_offset: first_range.start,
            included,
            excluded,
        })
    }
    pub fn iter(
        &self,
    ) -> SortedRangeIter<
        std::iter::Copied<std::slice::Iter<'_, TIncluded>>,
        std::iter::Copied<std::slice::Iter<'_, TExcluded>>,
    >
    where
        TIncluded: Copy + Into<u64>,
        TExcluded: Copy + Into<u64>,
    {
        SortedRangeIter {
            include: self.included.iter().copied(),
            excluded: self.excluded.iter().copied(),
            offset: self.initial_offset,
        }
    }
}

impl<TIncluded: Copy + Into<u64>, TExcluded: Copy + Into<u64>> IntoIterator
    for SortedRanges<TIncluded, TExcluded>
{
    type Item = RangeInclusive<u64>;
    type IntoIter = SortedRangeIter<std::vec::IntoIter<TIncluded>, std::vec::IntoIter<TExcluded>>;

    fn into_iter(self) -> Self::IntoIter {
        SortedRangeIter {
            include: self.included.into_iter(),
            excluded: self.excluded.into_iter(),
            offset: self.initial_offset,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct MetaRange<TMeta> {
    pub range: NonZeroRange<u64>,
    pub meta: TMeta,
}

impl<TMeta> MetaRange<TMeta> {
    pub fn copy_with_offset(&self, offset: i64) -> Self
    where
        TMeta: Copy,
    {
        Self {
            range: self.range.with_offset(offset),
            meta: self.meta,
        }
    }

    pub fn clone_with_offset(&self, offset: i64) -> Self
    where
        TMeta: Clone,
    {
        Self {
            range: self.range.with_offset(offset),
            meta: self.meta.clone(),
        }
    }
}

pub struct SortedRangeIter<TIncludedIter, TExcludedIter> {
    include: TIncludedIter,
    excluded: TExcludedIter,
    offset: u64,
}

impl<TIncluded: Iterator<Item: Copy + Into<u64>>, TExcluded: Iterator<Item: Copy + Into<u64>>>
    Iterator for SortedRangeIter<TIncluded, TExcluded>
{
    type Item = RangeInclusive<u64>;

    fn next(&mut self) -> Option<Self::Item> {
        let include = self.include.next()?;

        let out_range_end = self.offset + include.into();
        let range = self.offset..=(out_range_end - 1);
        if let Some(exclude) = self.excluded.next() {
            self.offset = out_range_end + exclude.into();
        };

        Some(range)
    }
}

impl<TIncluded: FusedIterator<Item: Copy + Into<u64>>, TExcluded: Iterator<Item: Copy + Into<u64>>>
    FusedIterator for SortedRangeIter<TIncluded, TExcluded>
{
}

impl<TIncluded: FusedIterator<Item: Copy + Into<u64>>, TExcluded: Iterator<Item: Copy + Into<u64>>>
    SortedStarts<u64> for SortedRangeIter<TIncluded, TExcluded>
{
}

impl<TIncluded: FusedIterator<Item: Copy + Into<u64>>, TExcluded: Iterator<Item: Copy + Into<u64>>>
    SortedDisjoint<u64> for SortedRangeIter<TIncluded, TExcluded>
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_with_initial_offset() {
        let encoded = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257]).unwrap();
        assert_eq!(
            vec![10u64..=19, 255u64..=256],
            encoded.iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn owned_iterator() {
        let encoded = SortedRanges::<u8, u8>::try_from_ordered_iter([10u32..20, 255..257]).unwrap();
        let collected: Vec<_> = encoded.into_iter().collect();
        assert_eq!(2, collected.len());
        assert_eq!(10u64..=19, collected[0]);
        assert_eq!(255u64..=256, collected[1]);
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
}
