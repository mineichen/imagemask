///
/// Working with ranges or collections/iterators of ranges
///
mod assert_sorted_iter;
mod flat_map_inplace;
mod merge_ordered_iter;
mod non_zero;

use std::{
    fmt::{Debug, Display},
    ops::Range,
};

pub use assert_sorted_iter::*;
pub use flat_map_inplace::*;
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

impl<TIncluded: Copy + Into<u64>, TExcluded: Copy + Into<u64>, TMeta> IntoIterator
    for NonEmptyOrderedRanges<TIncluded, TExcluded, Vec<TMeta>>
{
    type Item = MetaRange<TMeta>;
    type IntoIter = OwnedOrderedRangeIter<TIncluded, TExcluded, TMeta>;

    fn into_iter(self) -> Self::IntoIter {
        OwnedOrderedRangeIter {
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

pub struct OrderedRangeIter<'a, TIncluded, TExcluded, TMeta> {
    include: &'a [TIncluded],
    excluded: &'a [TExcluded],
    meta: &'a [TMeta],
    offset: u64,
}

pub struct OwnedOrderedRangeIter<TIncluded, TExcluded, TMeta> {
    include: std::vec::IntoIter<TIncluded>,
    excluded: std::vec::IntoIter<TExcluded>,
    meta: std::vec::IntoIter<TMeta>,
    offset: u64,
}

impl<TIncluded: Copy + Into<u64>, TExcluded: Copy + Into<u64>, TMeta> Iterator
    for OwnedOrderedRangeIter<TIncluded, TExcluded, TMeta>
{
    type Item = MetaRange<TMeta>;

    fn next(&mut self) -> Option<Self::Item> {
        let include = self.include.next()?;
        let meta = self.meta.next().expect("There must be more metadata");

        let out_range_end = self.offset + include.into();
        let out_range = unsafe { NonZeroRange::new_unchecked(self.offset..out_range_end) };

        if let Some(exclude) = self.excluded.next() {
            self.offset = out_range_end + exclude.into();
        }

        Some(MetaRange {
            range: out_range,
            meta,
        })
    }
}

impl<'a, TIncluded: Copy + Into<u64>, TExcluded: Copy + Into<u64>, TMeta> Iterator
    for OrderedRangeIter<'a, TIncluded, TExcluded, TMeta>
{
    type Item = MetaRange<&'a TMeta>;

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

        Some(MetaRange {
            range: out_range,
            meta,
        })
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
                MetaRange {
                    range: NonZeroRange::new(10..20),
                    meta: &"first"
                },
                MetaRange {
                    range: NonZeroRange::new(255..257),
                    meta: &"second"
                }
            ),
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
        assert_eq!(NonZeroRange::new(10..20), collected[0].range);
        assert_eq!("first", collected[0].meta);
        assert_eq!(NonZeroRange::new(255..257), collected[1].range);
        assert_eq!("second", collected[1].meta);
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
