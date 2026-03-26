use std::{cell::RefCell, marker::PhantomData, num::NonZeroU32, rc::Rc};

use crate::{CreateRange, SignedNonZeroable, UncheckedCast};

pub trait ImaskSet: Iterator + Sized {
    fn group_by_row_lending<R>(self, old_image_width: NonZeroU32) -> ChunkByRowRanges<Self, R>;
}

impl<I: Iterator> ImaskSet for I {
    /// # Panics
    /// If the previous RowIterator is kept when getting the next RowIterator
    fn group_by_row_lending<R>(self, old_image_width: NonZeroU32) -> ChunkByRowRanges<Self, R> {
        ChunkByRowRanges {
            shared: Rc::new(RefCell::new(Shared {
                source: self,
                pending_nextline: None,
            })),
            image_width: old_image_width,
            pending_lastrow: None,
            range: PhantomData,
        }
    }
}

/// result of
pub struct ChunkByRowRanges<T: Iterator, R> {
    shared: Rc<RefCell<Shared<T>>>,
    pending_lastrow: Option<T::Item>,
    image_width: NonZeroU32,
    range: PhantomData<R>,
}

struct Shared<T: Iterator> {
    source: T,
    pending_nextline: Option<T::Item>,
}

impl<T, R> Iterator for ChunkByRowRanges<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: SignedNonZeroable + UncheckedCast<usize>,
    usize: UncheckedCast<R::Item>,
{
    type Item = (usize, ChunkByRowRangesRowIter<T, R>);

    fn next(&mut self) -> Option<Self::Item> {
        // Panic if a SubIterator is still active
        debug_assert!(
            Rc::strong_count(&self.shared) == 1,
            "SubIterator must be consumed before requesting the next row"
        );

        let mut shared = self.shared.borrow_mut();
        let width = self.image_width.get() as usize;

        // Transfer pending_nextline from previous inner to pending_lastrow

        let range = shared
            .pending_nextline
            .take()
            .or_else(|| self.pending_lastrow.take())
            .or_else(|| shared.source.next())?;

        let start: R::Item = range.start();
        let start_usize: usize = UncheckedCast::cast_unchecked(start);
        let row = start_usize / width;
        let row_end_usize = (row + 1) * width;

        // If range crosses row boundary, store the remainder
        if UncheckedCast::<usize>::cast_unchecked(range.end()) > row_end_usize {
            let remaining: R::Item = UncheckedCast::cast_unchecked(
                UncheckedCast::<usize>::cast_unchecked(range.end()) - row_end_usize,
            );
            let remaining_len = unsafe { SignedNonZeroable::create_non_zero_unchecked(remaining) };
            let row_end: R::Item = UncheckedCast::cast_unchecked(row_end_usize);
            self.pending_lastrow = Some(R::new_debug_checked(row_end, remaining_len));
        }

        // Create the first range (possibly clipped to row boundary)
        let clip_end_usize = std::cmp::min(
            UncheckedCast::<usize>::cast_unchecked(range.end()),
            row_end_usize,
        );
        let first_len_val: R::Item = UncheckedCast::cast_unchecked(clip_end_usize - start_usize);
        let first_len = unsafe { SignedNonZeroable::create_non_zero_unchecked(first_len_val) };
        let first_range = R::new_debug_checked(start, first_len);

        Some((
            row,
            ChunkByRowRangesRowIter {
                shared: Rc::clone(&self.shared),
                pending: Some(first_range),
                phantom: PhantomData,
                next_line_start: UncheckedCast::cast_unchecked(row_end_usize),
            },
        ))
    }
}
pub struct ChunkByRowRangesRowIter<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: SignedNonZeroable + UncheckedCast<usize>,
    usize: UncheckedCast<R::Item>,
{
    shared: Rc<RefCell<Shared<T>>>,
    pending: Option<T::Item>,
    phantom: PhantomData<R>,
    next_line_start: R::Item,
}

impl<T, R> Iterator for ChunkByRowRangesRowIter<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: SignedNonZeroable + UncheckedCast<usize>,
    usize: UncheckedCast<R::Item>,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        let range = if let Some(p) = self.pending.take() {
            p
        } else {
            let mut shared = self.shared.borrow_mut();
            let Some(r) = shared.source.next() else {
                return None;
            };
            if UncheckedCast::<usize>::cast_unchecked(r.start())
                >= UncheckedCast::<usize>::cast_unchecked(self.next_line_start)
            {
                shared.pending_nextline = Some(r);
                return None;
            }
            r
        };

        // If range crosses the row boundary, clip it
        if UncheckedCast::<usize>::cast_unchecked(range.end())
            > UncheckedCast::<usize>::cast_unchecked(self.next_line_start)
        {
            let mut shared = self.shared.borrow_mut();
            let remaining: R::Item = UncheckedCast::cast_unchecked(
                UncheckedCast::<usize>::cast_unchecked(range.end())
                    - UncheckedCast::<usize>::cast_unchecked(self.next_line_start),
            );
            let remaining_len = unsafe { SignedNonZeroable::create_non_zero_unchecked(remaining) };
            shared.pending_nextline =
                Some(R::new_debug_checked(self.next_line_start, remaining_len));

            let clip_len_val: R::Item = UncheckedCast::cast_unchecked(
                UncheckedCast::<usize>::cast_unchecked(self.next_line_start)
                    - UncheckedCast::<usize>::cast_unchecked(range.start()),
            );
            let clip_len = unsafe { SignedNonZeroable::create_non_zero_unchecked(clip_len_val) };
            Some(R::new_debug_checked(range.start(), clip_len))
        } else {
            Some(range)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use super::*;

    #[test]
    fn split_without_linebreak() {
        let source = [0..4, 5..10, 11..20];
        let sums = source
            .into_iter()
            .group_by_row_lending::<Range<usize>>(10.try_into().unwrap())
            .map(|(row, i)| (row, i.map(|x| x.len()).sum::<usize>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((0, 9), (1, 9)));
    }
    #[test]
    fn split_with_linebreak() {
        let source = [0..20];
        let sums = source
            .into_iter()
            .group_by_row_lending::<Range<usize>>(10.try_into().unwrap())
            .map(|(row, i)| (row, i.map(|x| x.len()).sum::<usize>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((0, 10), (1, 10)));
    }
}
