use std::{
    fmt::Debug,
    ops::{Add, Div, Mul, Rem, Sub},
};

use crate::{CreateRange, ImageDimension, Rect, UncheckedCast};

struct RoiIter<TIter: Iterator<Item: CreateRange>> {
    source: Rect<u32>,
    target: Rect<u32>,
    parent: TIter,
    state: State<<TIter::Item as CreateRange>::Item>,
}

enum State<T> {
    WithinNotPending(StateWitin<T>),
}

impl<TIter> RoiIter<TIter>
where
    TIter: Iterator<Item: CreateRange> + ImageDimension,
    u32: UncheckedCast<<TIter::Item as CreateRange>::Item>,
{
    pub fn new(parent: TIter, target: Rect<u32>) -> Self {
        let source = parent.bounds();
        let stride = source.width.get() - target.width.get();
        let s_offset = source.x + source.y * source.width.get();
        let t_offset = target.x + target.y * target.width.get();
        Self {
            parent,
            source,
            target,
            state: State::WithinNotPending(StateWitin {
                stride: stride.cast_unchecked(),
                offset: (t_offset - s_offset).cast_unchecked(),
            }),
        }
    }
}

impl<TIter> Iterator for RoiIter<TIter>
where
    TIter: Iterator<
        Item: CreateRange<
            Item: Div<Output = <TIter::Item as CreateRange>::Item>
                      + Sub<Output = <TIter::Item as CreateRange>::Item>
                      + Mul<Output = <TIter::Item as CreateRange>::Item>
                      + Rem<Output = <TIter::Item as CreateRange>::Item>
                      + PartialEq
                      + Copy
                      + Ord
                      + Debug,
        > + Debug,
    >,
    u32: UncheckedCast<<TIter::Item as CreateRange>::Item>,
{
    type Item = TIter::Item;

    fn next(&mut self) -> Option<Self::Item> {
        match &self.state {
            State::WithinNotPending(s) => {
                loop {
                    let x = self.parent.next()?;
                    let start = x.start();
                    let end = x.end();

                    if end < s.offset {
                        continue;
                    }
                    println!(
                        "From src: {x:?}, offset: {:?}, stride: {:?}",
                        s.offset, s.stride
                    );
                    let start_row = start / self.source.width.get().cast_unchecked();
                    let start_col = start % self.source.width.get().cast_unchecked();
                    let start_line = start - start_col;

                    let end_row = end / self.source.width.get().cast_unchecked();
                    let new_start = start - s.stride * start_row;
                    let new_end = end - s.stride * end_row;
                    if start_row == end_row {
                        return Some(TIter::Item::new_debug_checked_zeroable(new_start, new_end));
                    } else {
                        todo!()
                        // Some(TIter::Item::new_debug_checked_zeroable(
                        //     start,
                        //     end.min(start),
                        // ))
                    }
                }
            }
        }
    }
}

impl<T: Iterator<Item: CreateRange>> ImageDimension for RoiIter<T> {
    fn bounds(&self) -> Rect<u32> {
        todo!()
    }

    fn width(&self) -> std::num::NonZero<u32> {
        todo!()
    }
}

struct StateWitin<T> {
    offset: T,
    stride: T,
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use crate::ImaskSet;

    use super::*;

    const NON_ZERO_80: NonZeroU32 = NonZeroU32::new(80).unwrap();
    const NON_ZERO_90: NonZeroU32 = NonZeroU32::new(90).unwrap();
    const NON_ZERO_99: NonZeroU32 = NonZeroU32::new(99).unwrap();
    const NON_ZERO_100: NonZeroU32 = NonZeroU32::new(100).unwrap();

    #[test]
    fn within_single_line() {
        let meaningless_offset = 100u32;
        let source = Rect::new(meaningless_offset, 0, NON_ZERO_100, NON_ZERO_100);
        let target = Rect::new(meaningless_offset + 10, 1, NON_ZERO_80, NON_ZERO_99);
        #[rustfmt::skip]
        let iter = RoiIter::new(
            [
                0..1, 5..11u64, 30..40, 85..90, // all ignored
                101..103, 110..112, 189..190, 192..195, // Borders
                209..210, 290..291, // Just outside
                330..340
            ].with_roi(source),
            target,
        );
        assert_eq!(vec!(0..2, 89..90, 270..280), iter.collect::<Vec<_>>());
    }
}
