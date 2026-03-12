use std::{any::type_name, fmt::Debug, ops::RangeInclusive};

pub struct RangeToOffsetsIter<TIter, TIncluded, TExcluded> {
    iter: TIter,
    prev_end: u64,
    _phantom: std::marker::PhantomData<(TIncluded, TExcluded)>,
}

impl<TIter, TIncluded, TExcluded> RangeToOffsetsIter<TIter, TIncluded, TExcluded> {
    pub fn new(iter: TIter) -> Self {
        Self {
            iter,
            prev_end: 0,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<TIter, TIncluded, TExcluded> Iterator for RangeToOffsetsIter<TIter, TIncluded, TExcluded>
where
    TIter: Iterator<Item = RangeInclusive<u64>>,
    TIncluded: TryFrom<u64, Error: Debug>,
    TExcluded: TryFrom<u64, Error: Debug>,
{
    type Item = (TExcluded, TIncluded);

    fn next(&mut self) -> Option<Self::Item> {
        let range = self.iter.next()?;
        let (start, end) = range.into_inner();
        let len = end - start + 1;
        let gap = start - self.prev_end;
        self.prev_end = start + len;
        let excluded = gap.try_into().unwrap_or_else(|_| {
            panic!(
                "Gap of {} is too large to fit into {}",
                gap,
                type_name::<TExcluded>()
            );
        });
        let included = len.try_into().unwrap_or_else(|_| {
            panic!(
                "Range length {} is too large to fit into {}",
                len,
                type_name::<TIncluded>()
            );
        });
        Some((excluded, included))
    }
}
