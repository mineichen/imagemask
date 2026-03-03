use std::{
    fmt::{Debug, Display},
    iter::FusedIterator,
    ops::{Range, RangeInclusive},
};

use range_set_blaze::{SortedDisjointMap, SortedStartsMap, ValueRef};

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
pub struct NonEmptyOrderedRanges<TIncluded, TExcluded, TMeta> {
    initial_offset: u64,
    included: Vec<TIncluded>,
    excluded: Vec<TExcluded>,
    meta: TMeta,
}
impl<TIncluded, TExcluded, TMeta> Debug for NonEmptyOrderedRanges<TIncluded, TExcluded, TMeta> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NonEmptyOrderedRanges")
            .field("range_count", &self.included.len())
            .finish()
    }
}

impl<TIncluded, TExcluded, TMeta> NonEmptyOrderedRanges<TIncluded, TExcluded, Vec<TMeta>> {
    pub fn new<TRange>(r: NonZeroRange<TRange>, meta: TMeta) -> Self
    where
        TRange: Into<u64> + Into<TIncluded> + Copy + std::ops::Sub<Output = TRange>,
    {
        let len = r.len().into();
        Self {
            initial_offset: r.start.into(),
            included: vec![len],
            excluded: Vec::new(),
            meta: vec![meta],
        }
    }
    pub fn try_from_ordered_iter<TRange>(
        iter: impl IntoIterator<Item = (Range<TRange>, TMeta)>,
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

        let mut iter = iter.into_iter().map(|(range, meta)| {
            let start = range.start.into();
            let end = range.end.into();
            create_checked::<TIncluded>(start, end).map(|x| (start..end, x, meta))
        });
        let Some((first_range, first_len, first_meta)) = iter.next().transpose()? else {
            return Err("Requires at least one item".into());
        };
        let mut included = Vec::<TIncluded>::with_capacity(iter.size_hint().0);
        let mut excluded = Vec::<TExcluded>::with_capacity(iter.size_hint().0);
        let mut meta = Vec::<TMeta>::with_capacity(iter.size_hint().0);

        included.push(first_len);
        meta.push(first_meta);
        let mut cur_pos = first_range.end;
        for x in iter {
            let (next_range, next_len, next_meta) = x?;
            excluded.push(create_checked(cur_pos, next_range.start)?);
            included.push(next_len);
            meta.push(next_meta);
            cur_pos = next_range.end;
        }

        Ok(Self {
            initial_offset: first_range.start,
            included,
            excluded,
            meta,
        })
    }
    pub fn iter(
        &self,
    ) -> OrderedRangeIter<
        std::iter::Copied<std::slice::Iter<'_, TIncluded>>,
        std::iter::Copied<std::slice::Iter<'_, TExcluded>>,
        std::slice::Iter<'_, TMeta>,
    >
    where
        TIncluded: Copy + Into<u64>,
        TExcluded: Copy + Into<u64>,
    {
        OrderedRangeIter {
            include: self.included.iter().copied(),
            excluded: self.excluded.iter().copied(),
            meta: self.meta.iter(),
            offset: self.initial_offset,
        }
    }
}

impl<TIncluded: Copy + Into<u64>, TExcluded: Copy + Into<u64>, TMeta> IntoIterator
    for NonEmptyOrderedRanges<TIncluded, TExcluded, Vec<TMeta>>
{
    type Item = (RangeInclusive<u64>, TMeta);
    type IntoIter = OrderedRangeIter<
        std::vec::IntoIter<TIncluded>,
        std::vec::IntoIter<TExcluded>,
        std::vec::IntoIter<TMeta>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        OrderedRangeIter {
            include: self.included.into_iter(),
            excluded: self.excluded.into_iter(),
            meta: self.meta.into_iter(),
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

pub struct OrderedRangeIter<TIncludedIter, TExcludedIter, TMetaIter: Iterator> {
    include: TIncludedIter,
    excluded: TExcludedIter,
    meta: TMetaIter,
    offset: u64,
}

impl<
    TIncluded: Iterator<Item: Copy + Into<u64>>,
    TExcluded: Iterator<Item: Copy + Into<u64>>,
    TMeta: Iterator,
> Iterator for OrderedRangeIter<TIncluded, TExcluded, TMeta>
{
    type Item = (RangeInclusive<u64>, TMeta::Item);

    fn next(&mut self) -> Option<Self::Item> {
        let include = self.include.next()?;
        let Some(meta) = self.meta.next() else {
            unreachable!("There must be more metadata");
        };

        let out_range_end = self.offset + include.into();
        let range = self.offset..=(out_range_end - 1);
        if let Some(exclude) = self.excluded.next() {
            self.offset = out_range_end + exclude.into();
        };

        Some((range, meta))
    }
}

impl<
    TIncluded: FusedIterator<Item: Copy + Into<u64>>,
    TExcluded: Iterator<Item: Copy + Into<u64>>,
    TMeta: Iterator,
> FusedIterator for OrderedRangeIter<TIncluded, TExcluded, TMeta>
{
}

impl<
    TIncluded: FusedIterator<Item: Copy + Into<u64>>,
    TExcluded: Iterator<Item: Copy + Into<u64>>,
    TMeta: Iterator<Item: ValueRef>,
> SortedStartsMap<u64, TMeta::Item> for OrderedRangeIter<TIncluded, TExcluded, TMeta>
{
}

impl<
    TIncluded: FusedIterator<Item: Copy + Into<u64>>,
    TExcluded: Iterator<Item: Copy + Into<u64>>,
    TMeta: Iterator<Item: ValueRef>,
> SortedDisjointMap<u64, TMeta::Item> for OrderedRangeIter<TIncluded, TExcluded, TMeta>
{
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_with_initial_offset() {
        let encoded = NonEmptyOrderedRanges::<u8, u8, _>::try_from_ordered_iter([
            (10u32..20, "first"),
            (255..257, "second"),
        ])
        .unwrap();
        assert_eq!(
            vec![(10u64..=19, &"first"), (255u64..=256, &"second")],
            encoded.iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn owned_iterator() {
        let encoded = NonEmptyOrderedRanges::<u8, u8, _>::try_from_ordered_iter([
            (10u32..20, "first".to_string()),
            (255..257, "second".to_string()),
        ])
        .unwrap();
        let collected: Vec<_> = encoded.into_iter().collect();
        assert_eq!(2, collected.len());
        assert_eq!(10u64..=19, collected[0].0);
        assert_eq!("first", collected[0].1);
        assert_eq!(255u64..=256, collected[1].0);
        assert_eq!("second", collected[1].1);
    }

    #[test]
    fn assert_big_gap_causes_error() {
        let error = NonEmptyOrderedRanges::<u16, u8, _>::try_from_ordered_iter([
            (10u32..20, "first"),
            (276..280, "second"),
        ])
        .unwrap_err();
        assert!(error.contains("out of range"), "{error}");
    }

    #[test]
    fn assert_big_ranges_cause_error() {
        let error =
            NonEmptyOrderedRanges::<u8, u16, _>::try_from_ordered_iter([(10u32..280, "first")])
                .unwrap_err();
        assert!(error.contains("out of range"), "{error}");
    }
    #[test]
    fn zero_ranges_cause_error() {
        let error =
            NonEmptyOrderedRanges::<u8, u8, _>::try_from_ordered_iter([(10u32..10, "first")])
                .unwrap_err();
        assert!(error.contains("> 10"), "{error}");
    }

    #[test]
    fn overlapping_cause_error() {
        let error = NonEmptyOrderedRanges::<u8, u8, _>::try_from_ordered_iter([
            (10u32..12, "first"),
            (11..12, "second"),
        ])
        .unwrap_err();
        assert!(error.contains("> 12"), "{error}");
    }
}
