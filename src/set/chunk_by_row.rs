use std::{cell::RefCell, marker::PhantomData, rc::Rc};

use crate::{CreateRange, SignedNonZeroable};

/// result of
pub struct ChunkByRowRanges<T: Iterator, R: CreateRange<Item: SignedNonZeroable>> {
    shared: Rc<RefCell<Shared<T>>>,
    image_width: <R::Item as SignedNonZeroable>::NonZero,
    range: PhantomData<R>,
}

impl<T: Iterator, R: CreateRange<Item: SignedNonZeroable>> ChunkByRowRanges<T, R> {
    /// # Panics
    /// If the previous RowIterator is kept when getting the next RowIterator
    pub(crate) fn new(source: T, old_image_width: <R::Item as SignedNonZeroable>::NonZero) -> Self {
        ChunkByRowRanges {
            shared: Rc::new(RefCell::new(Shared {
                source,
                pending_nextline: None,
            })),
            image_width: old_image_width,
            range: PhantomData,
        }
    }
}

struct Shared<T: Iterator> {
    source: T,
    pending_nextline: Option<T::Item>,
}

impl<T, R> Iterator for ChunkByRowRanges<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: Copy
        + Ord
        + std::ops::Add<Output = R::Item>
        + std::ops::Sub<Output = R::Item>
        + std::ops::Div<Output = R::Item>
        + std::ops::Mul<Output = R::Item>,
{
    type Item = (R::Item, ChunkByRowRangesRowIter<T, R>);

    fn next(&mut self) -> Option<Self::Item> {
        debug_assert!(
            Rc::strong_count(&self.shared) == 1,
            "The active SubIterator must be dropped before requesting the next row"
        );

        let mut shared = self.shared.borrow_mut();
        let width: R::Item = self.image_width.into();

        let range = shared
            .pending_nextline
            .take()
            .or_else(|| shared.source.next())?;

        let start = range.start();
        let row = start / width;
        let next_line_start = start / width * width + width;

        Some((
            row,
            ChunkByRowRangesRowIter {
                shared: Rc::clone(&self.shared),
                pending: Some(range),
                phantom: PhantomData,
                next_line_start,
            },
        ))
    }
}
pub struct ChunkByRowRangesRowIter<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: Copy + Ord + std::ops::Sub<Output = R::Item>,
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
    R::Item: Copy + Ord + std::ops::Sub<Output = R::Item>,
{
    fn drop(&mut self) {
        let mut shared = self.shared.borrow_mut();
        if self.pending.take().is_some() && shared.pending_nextline.is_none() {
            for r in &mut shared.source {
                if r.start() >= self.next_line_start {
                    shared.pending_nextline = Some(r);
                    break;
                } else if r.end() > self.next_line_start {
                    let remaining_len = unsafe {
                        SignedNonZeroable::create_non_zero_unchecked(r.end() - self.next_line_start)
                    };
                    shared.pending_nextline =
                        Some(R::new_debug_checked(self.next_line_start, remaining_len));
                    break;
                }
            }
        }
    }
}

impl<T, R> Iterator for ChunkByRowRangesRowIter<T, R>
where
    T: Iterator<Item = R>,
    R: CreateRange,
    R::Item: Copy + Ord + std::ops::Sub<Output = R::Item>,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        let mut shared = self.shared.borrow_mut();
        let range = self.pending.take().or_else(|| {
            shared.source.next().and_then(|r| {
                if r.start() >= self.next_line_start {
                    shared.pending_nextline = Some(r);
                    return None;
                }
                Some(r)
            })
        })?;

        if range.end() > self.next_line_start {
            // # Safety
            // Checked in if...
            let remaining_len = unsafe {
                SignedNonZeroable::create_non_zero_unchecked(range.end() - self.next_line_start)
            };

            shared.pending_nextline =
                Some(R::new_debug_checked(self.next_line_start, remaining_len));

            // # Safety
            // Would return None above, if start >= self.next_line_start
            let clip_len = unsafe {
                SignedNonZeroable::create_non_zero_unchecked(self.next_line_start - range.start())
            };
            Some(R::new_debug_checked(range.start(), clip_len))
        } else {
            Some(range)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        num::{NonZero, NonZeroUsize},
        ops::Range,
    };

    use super::*;
    const NON_ZERO_TEN: NonZeroUsize = NonZero::new(10).unwrap();

    #[test]
    fn split_without_linebreak() {
        let source = [0..4, 5..10, 11..20];
        let sums = ChunkByRowRanges::<_, Range<usize>>::new(source.into_iter(), NON_ZERO_TEN)
            .map(|(row, i)| (row, i.collect::<Vec<_>>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((0, vec![0..4, 5..10]), (1, vec![11..20])));
    }
    #[test]
    fn split_with_linebreak() {
        let source = [0..20];
        let sums = ChunkByRowRanges::<_, Range<usize>>::new(source.into_iter(), NON_ZERO_TEN)
            .map(|(row, i)| (row, i.map(|x| x.len()).sum::<usize>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((0, 10), (1, 10)));
    }
    #[test]
    fn split_with_filtered() {
        let source = [0..3, 6..11, 12..20];
        let sums = ChunkByRowRanges::<_, Range<usize>>::new(source.into_iter(), NON_ZERO_TEN)
            .skip(1)
            .map(|(row, i)| (row, i.collect::<Vec<_>>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((1, vec![10..11, 12..20])));
    }
    #[test]
    fn split_with_filtered_empty() {
        let source = [0..3, 4..5, 6..11, 12..20];
        let sums = ChunkByRowRanges::<_, Range<usize>>::new(source.into_iter(), NON_ZERO_TEN)
            .skip(1)
            .map(|(row, i)| (row, i.map(|x| x.len()).sum::<usize>()))
            .collect::<Vec<_>>();
        assert_eq!(sums, vec!((1, 9)));
    }
}
