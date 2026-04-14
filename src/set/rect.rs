use std::{
    fmt::Debug,
    iter::{FusedIterator, Once},
    marker::PhantomData,
    ops::{Add, Mul},
};

use num_traits::Zero;

use crate::{CreateRange, ImageDimension, Rect, SignedNonZeroable};

pub struct RectIterator<T: SignedNonZeroable, R> {
    pub kind: RectIteratorKind<T, R>,
    width: T::NonZero,
    height: T::NonZero,
}

#[derive(Clone)]
pub enum RectIteratorKind<T: SignedNonZeroable, R> {
    FullWidth(Once<R>),
    PartialWidth(PartialWidthRectIterator<T, R>),
}

impl<R> ImageDimension for RectIterator<u32, R> {
    fn width(&self) -> std::num::NonZero<u32> {
        self.width
    }

    fn bounds(&self) -> crate::Rect<u32> {
        Rect {
            x: 0,
            y: 0,
            width: self.width,
            height: self.height,
        }
    }
}

impl<T, R> RectIterator<T, R>
where
    T: SignedNonZeroable<NonZero: PartialOrd>
        + Mul<Output = T>
        + Add<Output = T>
        + Copy
        + Zero
        + Debug
        + PartialEq,
    R: CreateRange<Item = T>,
{
    pub fn new(
        x: T,
        y: T,
        width: T::NonZero,
        height: T::NonZero,
        global_width: T::NonZero,
    ) -> Self {
        debug_assert!(width <= global_width);
        let kind = if width < global_width {
            RectIteratorKind::PartialWidth(PartialWidthRectIterator::new(
                x,
                y,
                width,
                height,
                global_width,
            ))
        } else {
            debug_assert_eq!(x, T::zero(), "x must be zero for full width ranges");
            let start = y * global_width.into();
            let len = T::create_non_zero(height.into() * global_width.into())
                .expect("Only happens on overflow");
            RectIteratorKind::FullWidth(std::iter::once(R::new_debug_checked(start, len)))
        };
        Self {
            kind,
            width: global_width,
            height: T::create_non_zero(height.into() + y).unwrap(),
        }
    }
}

impl<T, R> Iterator for RectIterator<T, R>
where
    T: Add<Output = T> + Copy + SignedNonZeroable + PartialOrd,
    R: CreateRange<Item = T>,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        self.kind.next()
    }
}
impl<T, R> Iterator for RectIteratorKind<T, R>
where
    T: Add<Output = T> + Copy + SignedNonZeroable + PartialOrd,
    R: CreateRange<Item = T>,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::FullWidth(x) => x.next(),
            Self::PartialWidth(x) => x.next(),
        }
    }
}
impl<T: Add<Output = T> + Copy + SignedNonZeroable + PartialOrd, R: CreateRange<Item = T>>
    FusedIterator for RectIterator<T, R>
{
}

#[derive(Clone)]
pub struct PartialWidthRectIterator<T: SignedNonZeroable, R> {
    start_index: T,
    end_index: T,
    width: T::NonZero,
    image_width: T::NonZero,
    _range: PhantomData<R>,
}

impl<T, R> PartialWidthRectIterator<T, R>
where
    T: SignedNonZeroable<NonZero: PartialOrd> + Mul<Output = T> + Add<Output = T> + Copy,
{
    pub fn new(
        x: T,
        y: T,
        width: T::NonZero,
        height: T::NonZero,
        global_width: T::NonZero,
    ) -> Self {
        debug_assert!(width < global_width);
        let start_index = x + y * global_width.into();
        let end_index = start_index + height.into() * global_width.into();
        Self {
            start_index,
            end_index,
            width,
            image_width: global_width,
            _range: PhantomData,
        }
    }
}

impl<T, R> Iterator for PartialWidthRectIterator<T, R>
where
    T: Add<Output = T> + Copy + SignedNonZeroable + PartialOrd,
    R: CreateRange<Item = T>,
{
    type Item = R;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start_index < self.end_index {
            let r = R::new_debug_checked(self.start_index, self.width);
            self.start_index = self.start_index + self.image_width.into();
            Some(r)
        } else {
            None
        }
    }
}
impl<T: Add<Output = T> + Copy + SignedNonZeroable + PartialOrd, R: CreateRange<Item = T>>
    FusedIterator for PartialWidthRectIterator<T, R>
{
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_interop {
    use std::ops::{RangeInclusive, Sub};

    use num_traits::One;
    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};

    use super::*;

    impl<T> SortedStarts<T> for RectIterator<T, RangeInclusive<T>> where
        T: Add<Output = T> + Sub<Output = T> + Integer + One + SignedNonZeroable + PartialOrd
    {
    }
    impl<T: Add<Output = T> + Sub<Output = T> + Integer + One + SignedNonZeroable + PartialOrd>
        SortedDisjoint<T> for PartialWidthRectIterator<T, RangeInclusive<T>>
    {
    }
    impl<T> SortedStarts<T> for PartialWidthRectIterator<T, RangeInclusive<T>> where
        T: Add<Output = T> + Sub<Output = T> + Integer + One + SignedNonZeroable + PartialOrd
    {
    }
    impl<T> SortedDisjoint<T> for RectIterator<T, RangeInclusive<T>> where
        T: Add<Output = T> + Sub<Output = T> + Integer + One + SignedNonZeroable + PartialOrd
    {
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use super::*;
    const NON_ZERO_5: NonZeroU16 = NonZeroU16::new(5).unwrap();
    const NON_ZERO_10: NonZeroU16 = NonZeroU16::new(10).unwrap();

    #[test]
    fn simple_range() {
        let x = RectIterator::new(2u16, 4, NON_ZERO_5, NON_ZERO_5, NON_ZERO_10);
        assert_eq!(
            vec!(42..47, 52..57, 62..67, 72..77, 82..87),
            x.collect::<Vec<_>>()
        )
    }

    #[test]
    fn full_width_range() {
        let x = RectIterator::new(0u16, 2, NON_ZERO_5, NON_ZERO_5, NON_ZERO_5);
        assert_eq!(vec!(10..35), x.collect::<Vec<_>>());
    }
}
