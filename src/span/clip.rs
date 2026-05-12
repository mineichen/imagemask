use std::fmt::Debug;
use std::ops::Add;

use crate::{ImageDimension, NonZeroRange, Rect, SignedNonZeroable, Span};

pub struct ClipSpanIter<TIter, T: SignedNonZeroable> {
    parent: TIter,
    bounds: Rect<T>,
}

impl<TIter, T: SignedNonZeroable> ClipSpanIter<TIter, T> {
    pub fn new(parent: TIter, bounds: Rect<T>) -> Self {
        Self { parent, bounds }
    }
}

impl<TIter: ImageDimension, T: SignedNonZeroable> ImageDimension for ClipSpanIter<TIter, T> {
    fn bounds(&self) -> Rect<u32> {
        self.parent.bounds()
    }

    fn width(&self) -> std::num::NonZero<u32> {
        self.parent.width()
    }
}

impl<TIter: Iterator<Item = Span<T>>, T: SignedNonZeroable + Ord + Debug + Add<Output = T> + Copy>
    Iterator for ClipSpanIter<TIter, T>
{
    type Item = Span<T>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut span = self.parent.next()?;
            if span.y < self.bounds.y {
                span = self.parent.find(|s| s.y >= self.bounds.y)?;
            } else if span.y >= self.bounds.len_y().into() {
                return None;
            }
            if let Ok(range) = NonZeroRange::try_from(
                span.x.start.max(self.bounds.x)..span.x.end.min(self.bounds.len_x().into()),
            ) {
                return Some(Span::new(range, span.y));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use crate::{ImaskSet, Rect};

    use super::*;

    const NON_ZERO_5: NonZeroU32 = NonZeroU32::new(5).unwrap();
    const NON_ZERO_10: NonZeroU32 = NonZeroU32::new(10).unwrap();
    const NON_ZERO_100: NonZeroU32 = NonZeroU32::new(100).unwrap();

    #[test]
    fn smaller_bounds_do_crop() {
        let src = Rect::new(10u32, 10, NON_ZERO_10, NON_ZERO_10);
        let iter = src.into_spans();
        let bounds = Rect::new(12u32, 12, NON_ZERO_5, NON_ZERO_5);
        let expected = bounds.into_spans().collect::<Vec<_>>();
        let clipped = ClipSpanIter::new(iter, bounds).collect::<Vec<_>>();
        assert_eq!(expected, clipped);
    }
    #[test]
    fn bigger_bounds_have_no_effect() {
        let src = Rect::new(10u32, 10, NON_ZERO_10, NON_ZERO_10);
        let iter = src.into_spans();
        let expected = iter.clone().collect::<Vec<_>>();
        let bounds = Rect::new(0u32, 0, NON_ZERO_100, NON_ZERO_100);
        let clipped = ClipSpanIter::new(iter, bounds).collect::<Vec<_>>();
        assert_eq!(expected, clipped);
    }

    #[test]
    fn with_no_overlapping_parts() {
        let src = Rect::new(10u32, 10, NON_ZERO_10, NON_ZERO_10);
        let iter = src.into_spans();
        let expected = iter.clone().collect::<Vec<_>>();
        let bounds = Rect::new(0u32, 0, NON_ZERO_100, NON_ZERO_100);
        let clipped = ClipSpanIter::new(
            iter.union(
                Rect {
                    x: 100u32,
                    y: 10,
                    width: NON_ZERO_10,
                    height: NON_ZERO_10,
                }
                .into_spans(),
            ),
            bounds,
        )
        .collect::<Vec<_>>();
        assert_eq!(expected, clipped);
    }
}
