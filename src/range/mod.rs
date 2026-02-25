///
/// Working with ranges or collections/iterators of ranges
///
mod assert_sorted_iter;
mod merge_ordered_iter;
mod non_zero;

use std::{
    fmt::{Debug, Display},
    ops::Range,
};

pub use assert_sorted_iter::*;
pub use merge_ordered_iter::*;
pub use non_zero::*;

#[derive(Debug, Eq, PartialEq)]
pub struct OrderedRangeItem<TMeta> {
    pub range: NonZeroRange<u32>,
    pub meta: TMeta,
    pub priority: u32,
}

impl<TMeta> OrderedRangeItem<TMeta> {
    pub fn comparator(&self) -> (u32, u32) {
        (self.range.start, u32::MAX - self.priority)
    }
}

/// Represents areas on images. It's designed to efficiently support various image sizes.
/// Both, TIncluded and TExcluded are expected to always be > 0. Use non-zero signed types
/// Included represents the number of pixels to include, excluded encodes the gap between two included ranges
///
/// Included.len() = excluded.len() + 1
///
/// Meta is expected to be indexable for each included range
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
            initial_offset: first_range.start.into(),
            included,
            excluded,
            meta,
        })
    }
    pub fn iter(&self) -> OrderedRangeIter<'_, TIncluded, TExcluded, TMeta> {
        OrderedRangeIter {
            include: &self.included,
            excluded: &self.excluded,
            meta: &self.meta,
            offset: self.initial_offset,
        }
    }
}

pub struct OrderedRangeIter<'a, TIncluded, TExcluded, TMeta> {
    include: &'a [TIncluded],
    excluded: &'a [TExcluded],
    meta: &'a [TMeta],
    offset: u64,
}

impl<'a, TIncluded: Copy + Into<u64>, TExcluded: Copy + Into<u64>, TMeta> Iterator
    for OrderedRangeIter<'a, TIncluded, TExcluded, TMeta>
{
    type Item = (NonZeroRange<u64>, &'a TMeta);

    fn next(&mut self) -> Option<Self::Item> {
        let Some((&include, rest_include)) = self.include.split_first() else {
            return None;
        };
        let Some((meta, rest_meta)) = self.meta.split_first() else {
            unreachable!("There must be more metadata");
        };

        self.include = rest_include;
        self.meta = rest_meta;

        let out_range_end = self.offset + include.into();
        // Checked during construction, that start < end
        let out_range = unsafe { NonZeroRange::new_unchecked(self.offset..out_range_end) };
        if let Some((&exclude, rest_exclude)) = self.excluded.split_first() {
            self.offset = out_range_end + exclude.into();
            self.excluded = rest_exclude;
        };

        Some((out_range, meta))
    }
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
            vec!(
                (NonZeroRange::new(10..20), &"first"),
                (NonZeroRange::new(255..257), &"second")
            ),
            encoded.iter().collect::<Vec<_>>()
        );
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
