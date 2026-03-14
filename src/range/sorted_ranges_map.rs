use std::{
    cell::RefCell,
    collections::VecDeque,
    fmt::{Debug, Display},
    iter::FusedIterator,
    marker::PhantomData,
    num::NonZero,
    ops::{Range, RangeInclusive},
    rc::Rc,
};

use num_traits::Zero;

use crate::{CreateRange, NonZeroRange, SignedNonZeroable};

#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(feature = "rkyv", derive(rkyv::Archive))]
pub struct SortedRangesMap<TIncluded, TExcluded, TMeta> {
    included: Vec<TIncluded>,
    excluded: Vec<TExcluded>,
    meta: TMeta,
}
impl<TIncluded, TExcluded, TMeta> Debug for SortedRangesMap<TIncluded, TExcluded, TMeta> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NonEmptyOrderedRanges")
            .field("range_count", &self.included.len())
            .finish()
    }
}

impl<TIncluded, TExcluded, TMeta> SortedRangesMap<TIncluded, TExcluded, Vec<TMeta>> {
    pub fn new<TRange>(r: NonZeroRange<TRange>, meta: TMeta) -> Self
    where
        TRange: Into<TIncluded> + Into<TExcluded> + Copy + std::ops::Sub<Output = TRange>,
    {
        Self {
            included: vec![r.len().into()],
            excluded: vec![r.start.into()],
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
        let initial_offset = TExcluded::try_from(first_range.start).map_err(|e| e.to_string())?;

        let mut included = Vec::<TIncluded>::with_capacity(iter.size_hint().0 + 1);
        let mut excluded = Vec::<TExcluded>::with_capacity(iter.size_hint().0 + 1);
        let mut meta = Vec::<TMeta>::with_capacity(iter.size_hint().0 + 1);

        included.push(first_len);
        excluded.push(initial_offset);
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
            included,
            excluded,
            meta,
        })
    }
    pub fn iter<T: CreateRange<Item: Zero>>(
        &self,
    ) -> SortedRangesMapIter<
        std::iter::Copied<std::slice::Iter<'_, TIncluded>>,
        std::iter::Copied<std::slice::Iter<'_, TExcluded>>,
        std::slice::Iter<'_, TMeta>,
        T,
    >
    where
        TIncluded: Copy + Into<u64>,
        TExcluded: Copy + Into<u64>,
    {
        SortedRangesMapIter {
            include: self.included.iter().copied(),
            excluded: self.excluded.iter().copied(),
            meta: self.meta.iter(),
            offset: Zero::zero(),
            _out: PhantomData,
        }
    }

    #[allow(clippy::len_without_is_empty, reason = "is_empty would always be true")]
    pub fn len(&self) -> usize {
        self.included.len()
    }

    pub fn len_nonzero(&self) -> NonZero<usize> {
        NonZero::new(self.included.len())
            .expect("Constructors make sure, there is always at least one Range")
    }

    pub fn iter_owned<T: CreateRange<Item: Zero>>(
        self,
    ) -> SortedRangesMapIter<
        std::vec::IntoIter<TIncluded>,
        std::vec::IntoIter<TExcluded>,
        std::vec::IntoIter<TMeta>,
        T,
    > {
        SortedRangesMapIter {
            include: self.included.into_iter(),
            excluded: self.excluded.into_iter(),
            meta: self.meta.into_iter(),
            offset: Zero::zero(),
            _out: PhantomData,
        }
    }

    pub fn map_inplace<TIter, TFun>(self, f: TFun) -> Option<Self>
    where
        TIter: Iterator<Item = (RangeInclusive<u64>, TMeta)>,
        TFun: FnOnce(SourceIteratorMap<TIncluded, TExcluded, TMeta>) -> TIter,
        TIncluded: TryFrom<u64, Error: Debug> + Clone,
        TExcluded: TryFrom<u64, Error: Debug> + Clone,
        TMeta: Clone,
    {
        let original_len = self.included.len();
        let cell = Rc::new(RefCell::new((self, 0usize)));

        let source = SourceIteratorMap {
            cell: cell.clone(),
            offset: 0,
            original_len,
        };

        let items = f(source);
        let offsets_iter = RangeToOffsetsIterMap::<_, TIncluded, TExcluded, TMeta>::new(items);
        let mut cache: VecDeque<(TExcluded, TIncluded, TMeta)> = VecDeque::new();
        let mut write_pos = 0;

        let write_tuple =
            |col: &mut SortedRangesMap<_, _, Vec<_>>, (excl, incl, meta), write_pos: &mut usize| {
                if *write_pos < col.included.len() {
                    col.excluded[*write_pos] = excl;
                    col.included[*write_pos] = incl;
                    col.meta[*write_pos] = meta;
                } else {
                    col.excluded.push(excl);
                    col.included.push(incl);
                    col.meta.push(meta);
                }
                *write_pos += 1;
            };

        for tuple in offsets_iter {
            let mut x = cell.borrow_mut();
            let (read_pos, col) = (x.1, &mut x.0);
            if (write_pos < read_pos || read_pos >= original_len) && cache.is_empty() {
                write_tuple(col, tuple, &mut write_pos);
            } else {
                cache.push_back(tuple);
                while (write_pos < read_pos || read_pos >= original_len)
                    && let Some(t) = cache.pop_front()
                {
                    write_tuple(col, t, &mut write_pos)
                }
            }
        }

        let not_empty = {
            let mut x = cell.borrow_mut();
            let col = &mut x.0;
            while let Some(tuple) = cache.pop_front() {
                write_tuple(col, tuple, &mut write_pos);
            }

            col.included.truncate(write_pos);
            col.excluded.truncate(write_pos);
            col.meta.truncate(write_pos);
            !x.0.included.is_empty()
        };
        not_empty.then(move || {
            Rc::try_unwrap(cell)
                .expect("You are not allowed to move SourceIter outside the lambda")
                .into_inner()
                .0
        })
    }
}

pub struct RangeToOffsetsIterMap<TIter, TIncluded, TExcluded, TMeta> {
    iter: TIter,
    prev_end: u64,
    _phantom: PhantomData<(TIncluded, TExcluded, TMeta)>,
}

impl<TIter, TIncluded, TExcluded, TMeta> RangeToOffsetsIterMap<TIter, TIncluded, TExcluded, TMeta> {
    pub fn new(iter: TIter) -> Self {
        Self {
            iter,
            prev_end: 0,
            _phantom: PhantomData,
        }
    }
}

impl<TIter, TIncluded, TExcluded, TMeta> Iterator
    for RangeToOffsetsIterMap<TIter, TIncluded, TExcluded, TMeta>
where
    TIter: Iterator<Item = (RangeInclusive<u64>, TMeta)>,
    TIncluded: TryFrom<u64, Error: Debug>,
    TExcluded: TryFrom<u64, Error: Debug>,
{
    type Item = (TExcluded, TIncluded, TMeta);

    fn next(&mut self) -> Option<Self::Item> {
        let (range, meta) = self.iter.next()?;
        let (start, end) = range.into_inner();
        let len = end - start + 1;
        let gap = start - self.prev_end;
        self.prev_end = start + len;
        let excluded = gap.try_into().unwrap();
        let included = len.try_into().unwrap();
        Some((excluded, included, meta))
    }
}

pub struct SourceIteratorMap<TIncluded, TExcluded, TMeta> {
    cell: Rc<RefCell<(SortedRangesMap<TIncluded, TExcluded, Vec<TMeta>>, usize)>>,
    offset: u64,
    original_len: usize,
}

unsafe impl<TIncluded: Send, TExcluded: Send, TMeta: Send> Send
    for SourceIteratorMap<TIncluded, TExcluded, TMeta>
{
}

impl<TIncluded, TExcluded, TMeta> FusedIterator for SourceIteratorMap<TIncluded, TExcluded, TMeta>
where
    TIncluded: Copy + Into<u64>,
    TExcluded: Copy + Into<u64>,
    TMeta: Clone,
{
}

impl<TIncluded, TExcluded, TMeta> Iterator for SourceIteratorMap<TIncluded, TExcluded, TMeta>
where
    TIncluded: Copy + Into<u64>,
    TExcluded: Copy + Into<u64>,
    TMeta: Clone,
{
    type Item = (RangeInclusive<u64>, TMeta);

    fn next(&mut self) -> Option<Self::Item> {
        let mut x = self.cell.borrow_mut();
        let (col, read_pos) = &mut *x;
        if *read_pos >= self.original_len {
            return None;
        }
        let exclude = (*col.excluded.get(*read_pos)?).into();
        self.offset += exclude;

        let include = (*col.included.get(*read_pos)?).into();
        let out_range =
            RangeInclusive::new_debug_checked(self.offset, NonZero::new(include).unwrap());
        self.offset += include;

        let meta = col.meta[*read_pos].clone();
        *read_pos += 1;

        Some((out_range, meta))
    }
}

impl<TIncluded: Copy + Into<u64>, TExcluded: Copy + Into<u64>, TMeta> IntoIterator
    for SortedRangesMap<TIncluded, TExcluded, Vec<TMeta>>
{
    type Item = MetaRange<NonZeroRange<u64>, TMeta>;
    type IntoIter = SortedRangesMapIter<
        std::vec::IntoIter<TIncluded>,
        std::vec::IntoIter<TExcluded>,
        std::vec::IntoIter<TMeta>,
        NonZeroRange<u64>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        SortedRangesMapIter {
            include: self.included.into_iter(),
            excluded: self.excluded.into_iter(),
            meta: self.meta.into_iter(),
            offset: 0,
            _out: PhantomData,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
pub struct MetaRange<TRange, TMeta> {
    pub range: TRange,
    pub meta: TMeta,
}

impl<TRange, TMeta> From<(TRange, TMeta)> for MetaRange<TRange, TMeta> {
    fn from((range, meta): (TRange, TMeta)) -> Self {
        Self { range, meta }
    }
}

impl<TMeta> MetaRange<NonZeroRange<u64>, TMeta> {
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

pub struct SortedRangesMapIter<
    TIncludedIter,
    TExcludedIter,
    TMetaIter: Iterator,
    TRange: CreateRange,
> {
    include: TIncludedIter,
    excluded: TExcludedIter,
    meta: TMetaIter,
    offset: TRange::Item,
    _out: PhantomData<TRange>,
}

impl<
    TIncluded: Iterator<Item: Copy + TryInto<TRange::Item, Error: Debug>>,
    TExcluded: Iterator<Item: Copy + TryInto<TRange::Item, Error: Debug>>,
    TMeta: Iterator,
    TRange: CreateRange,
> Iterator for SortedRangesMapIter<TIncluded, TExcluded, TMeta, TRange>
where
    TRange::Item: TryFrom<TIncluded::Item, Error: Debug>
        + TryFrom<TExcluded::Item, Error: Debug>
        + SignedNonZeroable
        + Copy
        + std::ops::Add<Output = TRange::Item>,
{
    type Item = TRange::ListItem<TMeta::Item>;

    fn next(&mut self) -> Option<Self::Item> {
        let exclude = self.excluded.next()?.try_into().unwrap();
        self.offset = self.offset + exclude;

        let Some(include) = self.include.next() else {
            unreachable!("There must be more include");
        };
        let include = include.try_into().unwrap();
        let Some(meta) = self.meta.next() else {
            unreachable!("There must be more metadata");
        };

        let offset_item = TRange::Item::try_from(self.offset).expect("Cast shouldn't overflow");
        let len_item = TRange::Item::try_from(include).expect("Cast include shouldn't overflow");
        let out_range = TRange::new_debug_checked(offset_item, len_item.create_non_zero().unwrap());
        self.offset = self.offset + include;

        Some((out_range, meta).into())
    }
}

impl<TIncluded, TExcluded, TMeta, TRange> FusedIterator
    for SortedRangesMapIter<TIncluded, TExcluded, TMeta, TRange>
where
    TIncluded: FusedIterator<Item: Copy + Into<TRange::Item>>,
    TExcluded: Iterator<Item: Copy + Into<TRange::Item>>,
    TMeta: Iterator,
    TRange: CreateRange,
    TRange::Item: TryFrom<TIncluded::Item, Error: Debug>
        + TryFrom<TExcluded::Item, Error: Debug>
        + SignedNonZeroable
        + Copy
        + std::ops::Add<Output = TRange::Item>,
{
}
#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_0_5_interop {
    use super::*;
    use range_set_blaze_0_5::{Integer, SortedDisjointMap, SortedStartsMap, ValueRef};
    use std::ops::RangeInclusive;

    impl<TIncluded, TExcluded, TMeta, TRangeItem> SortedStartsMap<TRangeItem, TMeta::Item>
        for SortedRangesMapIter<TIncluded, TExcluded, TMeta, RangeInclusive<TRangeItem>>
    where
        TIncluded: FusedIterator<Item: Copy + Into<TRangeItem>>,
        TExcluded: Iterator<Item: Copy + Into<TRangeItem>>,
        TMeta: Iterator<Item: ValueRef>,
        TRangeItem: TryFrom<TIncluded::Item, Error: Debug>
            + TryFrom<TExcluded::Item, Error: Debug>
            + Copy
            + Integer
            + num_traits::One
            + SignedNonZeroable
            + std::ops::Sub<Output = TRangeItem>
            + std::ops::Add<Output = TRangeItem>,
    {
    }

    impl<TIncluded, TExcluded, TMeta, TRangeItem> SortedDisjointMap<TRangeItem, TMeta::Item>
        for SortedRangesMapIter<TIncluded, TExcluded, TMeta, std::ops::RangeInclusive<TRangeItem>>
    where
        TIncluded: FusedIterator<Item: Copy + Into<TRangeItem>>,
        TExcluded: Iterator<Item: Copy + Into<TRangeItem>>,
        TMeta: Iterator<Item: ValueRef>,
        TRangeItem: TryFrom<TIncluded::Item, Error: Debug>
            + TryFrom<TExcluded::Item, Error: Debug>
            + Copy
            + range_set_blaze_0_5::Integer
            + num_traits::One
            + SignedNonZeroable
            + std::ops::Sub<Output = TRangeItem>
            + std::ops::Add<Output = TRangeItem>,
    {
    }

    impl<TIncluded, TExcluded, TMeta> SortedStartsMap<u64, TMeta>
        for SourceIteratorMap<TIncluded, TExcluded, TMeta>
    where
        TIncluded: Copy + Into<u64>,
        TExcluded: Copy + Into<u64>,
        TMeta: ValueRef,
    {
    }

    impl<TIncluded, TExcluded, TMeta> SortedDisjointMap<u64, TMeta>
        for SourceIteratorMap<TIncluded, TExcluded, TMeta>
    where
        TIncluded: Copy + Into<u64>,
        TExcluded: Copy + Into<u64>,
        TMeta: ValueRef,
    {
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZero, ops::RangeInclusive};

    use super::*;

    #[cfg(feature = "range-set-blaze-0_5")]
    mod blaze {
        use super::*;
        use range_set_blaze_0_5::{SortedDisjointMap, ValueRef};
        #[derive(PartialEq, Eq, Clone, Debug)]
        struct MetaItem(&'static str);

        impl From<&'static str> for MetaItem {
            fn from(value: &'static str) -> Self {
                Self(value)
            }
        }

        impl ValueRef for MetaItem {
            type Target = MetaItem;

            fn into_value(self) -> Self::Target {
                self
            }
        }
        #[test]
        fn combine_owned() {
            //type MetaItem = String;
            let a = SortedRangesMap::<u8, u8, Vec<MetaItem>>::try_from_ordered_iter([
                (10u32..30, "a_first".into()),
                (42..50, "a_second".into()),
            ])
            .unwrap();
            let b = SortedRangesMap::<u8, u8, Vec<MetaItem>>::try_from_ordered_iter([
                (20u32..30, "b_first".into()),
                (41..45, "b_second".into()),
            ])
            .unwrap();

            let a_iter = a.iter_owned::<RangeInclusive<u64>>();
            let b_iter = b.iter_owned::<RangeInclusive<u64>>();
            let result = b_iter
                .union(a_iter)
                .map(|(r, m)| (*r.start()..(*r.end() + 1), m))
                .collect::<Vec<_>>();

            assert_eq!(
                vec![
                    (10u64..30, MetaItem::from("a_first")),
                    (41..42, MetaItem::from("b_second")),
                    (42..50, MetaItem::from("a_second"))
                ],
                result
            );
        }
        #[test]
        fn combine_inline() {
            use range_set_blaze_0_5::SortedDisjointMap;

            let a = SortedRangesMap::<u8, u8, Vec<MetaItem>>::try_from_ordered_iter([
                (10u32..30, "a_first".into()),
                (42..50, "a_second".into()),
            ])
            .unwrap();
            let b = SortedRangesMap::<u8, u8, Vec<MetaItem>>::try_from_ordered_iter([
                (20u32..30, "b_first".into()),
                (41..45, "b_second".into()),
            ])
            .unwrap();

            let b_iter = b.iter_owned::<RangeInclusive<u64>>();
            let a = a
                .map_inplace(|a_iter| {
                    b_iter
                        .union(a_iter)
                        .map(|(r, m)| (*r.start()..=(*r.end()), m))
                })
                .unwrap();

            assert_eq!(
                vec![
                    (10u64..30, MetaItem::from("a_first")),
                    (41..42, MetaItem::from("b_second")),
                    (42..50, MetaItem::from("a_second"))
                ],
                a.iter_owned::<Range<u64>>().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn ranges_starting_at_zero() {
        let map = SortedRangesMap::<u32, u32, Vec<&str>>::try_from_ordered_iter([
            (0u64..1, "first"),
            (5u64..6, "second"),
        ]);

        let map = map.unwrap();
        let collected: Vec<_> = map.iter::<std::ops::Range<u64>>().map(|x| x.0).collect();
        assert_eq!(vec![0u64..1, 5u64..6], collected);
    }

    #[test]
    fn range_with_initial_offset() {
        let encoded = SortedRangesMap::<u8, u8, _>::try_from_ordered_iter([
            (10u32..20, "first"),
            (255..257, "second"),
        ])
        .unwrap();
        assert_eq!(
            vec![(10u64..=19, &"first"), (255u64..=256, &"second")],
            encoded.iter::<RangeInclusive<u64>>().collect::<Vec<_>>()
        );
    }

    #[test]
    fn owned_iterator_inclusive() {
        let encoded = SortedRangesMap::<u8, u8, _>::try_from_ordered_iter([
            (10u32..20, "first".to_string()),
            (255..257, "second".to_string()),
        ])
        .unwrap();
        let collected: Vec<_> = encoded.iter_owned::<RangeInclusive<u64>>().collect();
        assert_eq!(10u64..=19, collected[0].0);
        assert_eq!("first", collected[0].1);
        assert_eq!(255u64..=256, collected[1].0);
        assert_eq!("second", collected[1].1);
        assert_eq!(2, collected.len());
    }
    #[test]
    fn owned_iterator() {
        let encoded = SortedRangesMap::<u8, u8, _>::try_from_ordered_iter([
            (10u32..20, "first".to_string()),
            (255..257, "second".to_string()),
        ])
        .unwrap();
        let collected: Vec<_> = encoded.into_iter().collect();
        assert_eq!(2, collected.len());
        assert_eq!(
            NonZeroRange::from_span(10, const { NonZero::new(10).unwrap() }),
            collected[0].range
        );
        assert_eq!("first", collected[0].meta);
        assert_eq!(
            NonZeroRange::from_span(255, const { NonZero::new(2).unwrap() },),
            collected[1].range
        );
        assert_eq!("second", collected[1].meta);
    }

    #[test]
    fn assert_big_gap_causes_error() {
        let error = SortedRangesMap::<u16, u8, _>::try_from_ordered_iter([
            (10u32..20, "first"),
            (276..280, "second"),
        ])
        .unwrap_err();
        assert!(error.contains("out of range"), "{error}");
    }

    #[test]
    fn assert_big_ranges_cause_error() {
        let error = SortedRangesMap::<u8, u16, _>::try_from_ordered_iter([(10u32..280, "first")])
            .unwrap_err();
        assert!(error.contains("out of range"), "{error}");
    }
    #[test]
    fn zero_ranges_cause_error() {
        let error = SortedRangesMap::<u8, u8, _>::try_from_ordered_iter([(10u32..10, "first")])
            .unwrap_err();
        assert!(error.contains("> 10"), "{error}");
    }

    #[test]
    fn overlapping_cause_error() {
        let error = SortedRangesMap::<u8, u8, _>::try_from_ordered_iter([
            (10u32..12, "first"),
            (11..12, "second"),
        ])
        .unwrap_err();
        assert!(error.contains("> 12"), "{error}");
    }

    #[test]
    fn split_combine() {
        let a = SortedRangesMap::<u8, u8, Vec<String>>::try_from_ordered_iter([
            (10u32..15, "a1".to_string()),
            (30..35, "a2".to_string()),
        ])
        .unwrap();

        let a = a
            .map_inplace(|iter| {
                iter.map(|(x, m)| {
                    let (start, end) = x.into_inner();
                    ((start + 5)..=(end + 5), m)
                })
            })
            .unwrap();

        assert_eq!(
            vec![(15u64..=19, "a1"), (35..=39, "a2")],
            a.iter::<RangeInclusive<u64>>()
                .map(|(r, m)| (r, m.as_str()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn split_when_collection_becomes_bigger() {
        let a = SortedRangesMap::<u8, u8, Vec<String>>::try_from_ordered_iter([
            (10u32..15, "first".to_string()),
            (30..35, "second".to_string()),
        ])
        .unwrap();

        let a = a
            .map_inplace(|iter| {
                iter.flat_map(|(x, m)| {
                    let with_offset = (*x.start() + 10)..=(*x.end() + 10);
                    [(x, m.clone()), (with_offset, format!("{}_offset", m))]
                })
            })
            .unwrap();

        assert_eq!(
            vec![
                (10u64..=14, "first"),
                (20..=24, "first_offset"),
                (30..=34, "second"),
                (40..=44, "second_offset")
            ],
            a.iter::<RangeInclusive<u64>>()
                .map(|(r, m)| (r, m.as_str()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn split_returns_none_when_empty() {
        let a = SortedRangesMap::<u8, u8, Vec<String>>::try_from_ordered_iter([(
            10u32..15,
            "test".to_string(),
        )])
        .unwrap();

        let result = a.map_inplace(|_| std::iter::empty());

        assert!(result.is_none());
    }
}
