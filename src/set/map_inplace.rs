use std::{
    cell::RefCell, collections::VecDeque, fmt::Debug, iter::FusedIterator, num::NonZero,
    ops::RangeInclusive, rc::Rc,
};

use crate::{CreateRange, ImageDimension, RangeToOffsetsIter, SortedRanges, UncheckedCast};

impl<TIncluded, TExcluded> SortedRanges<TIncluded, TExcluded> {
    /// Transform the ranges in-place using a closure.
    /// The closure receives a SourceIterator and returns an iterator of `RangeInclusive<u64>`.
    /// Returns Some(SortedRanges) if non-empty, None if empty.
    /// ```
    /// use std::ops::RangeInclusive;
    /// use imask::{Rect, SortedRanges, SourceIterator};
    /// use std::num::NonZero;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let bounds = Rect::new(0u32, 0, NonZero::new(1000u32).unwrap(), NonZero::new(1000u32).unwrap());
    /// let ranges = SortedRanges::<u16, u16>::try_from_ordered_iter([10u32..20, 30..45, 50..60], bounds)?;
    /// let ranges = ranges.map_inplace(|iter| {
    ///     iter.map(|x| {
    ///         let (start, end) = x.into_inner();
    ///         (start+5)..=(end + 5)
    ///     })
    /// }).expect("Should not be empty");
    /// assert_eq!(
    ///     vec!(15u64..25, 35..50, 55..65),
    ///     ranges.iter_owned::<std::ops::Range<u64>>().collect::<Vec<_>>()
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn map_inplace<TIter, TFun>(self, f: TFun) -> Option<Self>
    where
        TIter: Iterator<Item = RangeInclusive<u64>>,
        TFun: FnOnce(SourceIterator<TIncluded, TExcluded>) -> TIter,
        TIncluded: TryFrom<u64, Error: Debug> + Clone,
        TExcluded: TryFrom<u64, Error: Debug> + Clone,
    {
        let original_len = self.included.len();
        // Rc is required, because we cannot restrict TIter by the Lifetime of the FnOnce-argument
        // When working with pointers, it was difficult to forbid the Lambda use to std::mem::swap...
        // If this happens, `map_inplace` panics
        let cell = Rc::new(RefCell::new((self, 0usize)));

        let source = SourceIterator {
            cell: cell.clone(),
            offset: 0,
            original_len,
        };

        let items = f(source);
        let offsets_iter = RangeToOffsetsIter::<_, TIncluded, TExcluded>::new(items);
        let mut cache: VecDeque<(TExcluded, TIncluded)> = VecDeque::new();
        let mut write_pos = 0;

        let write_tuple = |col: &mut SortedRanges<_, _>, (excl, incl), write_pos: &mut usize| {
            if *write_pos < col.included.len() {
                col.excluded[*write_pos] = excl;
                col.included[*write_pos] = incl;
            } else {
                col.excluded.push(excl);
                col.included.push(incl);
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
                    && let Some(tuple) = cache.pop_front()
                {
                    write_tuple(col, tuple, &mut write_pos)
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
            !x.0.included.is_empty()
        };

        not_empty.then(move|| {
            Rc::try_unwrap(cell).expect("You mustn't move the SourceIterator outside the lambda provided to map_inplace").into_inner().0
        })
    }
}

pub struct SourceIterator<TIncluded, TExcluded> {
    cell: Rc<RefCell<(SortedRanges<TIncluded, TExcluded>, usize)>>,
    offset: u64,
    original_len: usize,
}

impl<TIncluded, TExcluded> FusedIterator for SourceIterator<TIncluded, TExcluded>
where
    TIncluded: UncheckedCast<u64>,
    TExcluded: UncheckedCast<u64>,
{
}

impl<TIncluded, TExcluded> ImageDimension for SourceIterator<TIncluded, TExcluded> {
    fn width(&self) -> NonZero<u32> {
        self.cell.borrow().0.bounds.width
    }
}

impl<TIncluded, TExcluded> Iterator for SourceIterator<TIncluded, TExcluded>
where
    TIncluded: UncheckedCast<u64>,
    TExcluded: UncheckedCast<u64>,
{
    type Item = RangeInclusive<u64>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut x = self.cell.borrow_mut();
        let (col, read_pos) = &mut *x;
        if *read_pos >= self.original_len {
            return None;
        }
        let exclude = (*col.excluded.get(*read_pos)?).cast_unchecked();
        self.offset += exclude;

        let include = (*col.included.get(*read_pos)?).cast_unchecked();
        let out_range =
            RangeInclusive::new_debug_checked(self.offset, NonZero::new(include).unwrap());
        self.offset += include;
        *read_pos += 1;

        Some(out_range)
    }
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_0_5_interop {
    use range_set_blaze_0_5::{SortedDisjoint, SortedStarts};

    use super::*;
    impl<TIncluded, TExcluded> SortedStarts<u64> for SourceIterator<TIncluded, TExcluded>
    where
        TIncluded: UncheckedCast<u64>,
        TExcluded: UncheckedCast<u64>,
    {
    }

    impl<TIncluded, TExcluded> SortedDisjoint<u64> for SourceIterator<TIncluded, TExcluded>
    where
        TIncluded: UncheckedCast<u64>,
        TExcluded: UncheckedCast<u64>,
    {
    }
}
