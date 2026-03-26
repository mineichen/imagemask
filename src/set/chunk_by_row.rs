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
            range: PhantomData,
        }
    }
}

/// result of
pub struct ChunkByRowRanges<T: Iterator, R> {
    shared: Rc<RefCell<Shared<T>>>,
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
            "The active SubIterator must be dropped before requesting the next row"
        );

        let mut shared = self.shared.borrow_mut();
        let width = self.image_width.get() as usize;

        let range = shared
            .pending_nextline
            .take()
            .or_else(|| shared.source.next())?;

        let start: R::Item = range.start();
        let start_usize: usize = UncheckedCast::cast_unchecked(start);
        let row = start_usize / width;
        let row_end_usize = (row + 1) * width;

        Some((
            row,
            ChunkByRowRangesRowIter {
                shared: Rc::clone(&self.shared),
                pending: Some(range),
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

impl<T, R> Drop for ChunkByRowRangesRowIter<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: SignedNonZeroable + UncheckedCast<usize>,
    usize: UncheckedCast<R::Item>,
{
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        if self.pending.take().is_some() && shared.pending_nextline.is_none() {
            // Inner was dropped before consuming its pending item.
            // Drain source items for this row so the outer doesn't re-yield this row.
            let next_line = self.next_line_start;
            loop {
                match shared.source.next() {
                    Some(r) => {
                        let start: usize = UncheckedCast::cast_unchecked(r.start());
                        let end: usize = UncheckedCast::cast_unchecked(r.end());
                        let row_end: usize = UncheckedCast::cast_unchecked(next_line);
                        if start >= row_end {
                            // Range starts at or after the next row
                            shared.pending_nextline = Some(r);
                            break;
                        } else if end > row_end {
                            // Range crosses the row boundary: clip and save remainder
                            let remaining: R::Item = UncheckedCast::cast_unchecked(end - row_end);
                            let remaining_len =
                                unsafe { SignedNonZeroable::create_non_zero_unchecked(remaining) };
                            shared.pending_nextline =
                                Some(R::new_debug_checked(next_line, remaining_len));
                            break;
                        }
                        // else: range is entirely within this row, discard
                    }
                    None => break,
                }
            }
        }
    }
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
        let mut shared = self.shared.borrow_mut();
        let range = self.pending.take().or_else(|| {
            shared.source.next().and_then(|r| {
                if UncheckedCast::<usize>::cast_unchecked(r.start())
                    >= UncheckedCast::<usize>::cast_unchecked(self.next_line_start)
                {
                    shared.pending_nextline = Some(r);
                    return None;
                }
                Some(r)
            })
        })?;

        // If range crosses the row boundary, clip it
        if UncheckedCast::<usize>::cast_unchecked(range.end())
            > UncheckedCast::<usize>::cast_unchecked(self.next_line_start)
        {
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
    #[test]
    fn split_with_filtered() {
        let source = [0..3, 6..11, 12..20];
        let sums = source
            .into_iter()
            .group_by_row_lending::<Range<usize>>(10.try_into().unwrap())
            .skip(1)
            .map(|(row, i)| (row, i.map(|x| x.len()).sum::<usize>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((1, 9)));
    }
    #[test]
    fn split_with_filtered_empty() {
        let source = [0..3, 4..5, 6..11, 12..20];
        let sums = source
            .into_iter()
            .group_by_row_lending::<Range<usize>>(10.try_into().unwrap())
            .skip(1)
            .map(|(row, i)| (row, i.map(|x| x.len()).sum::<usize>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((1, 9)));
    }
}
