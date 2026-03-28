use std::fmt::Debug;

use crate::{CreateRange, NonZeroRange, RectIterator, SignedNonZeroable};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rect<T: SignedNonZeroable> {
    pub x: T,
    pub y: T,
    pub width: T::NonZero,
    pub height: T::NonZero,
}

impl<T: SignedNonZeroable + Debug> Debug for Rect<T>
where
    T::NonZero: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rect")
            .field("x", &self.x)
            .field("y", &self.y)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl<T: SignedNonZeroable> Rect<T> {
    pub fn new(x: T, y: T, width: T::NonZero, height: T::NonZero) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn range_x(&self) -> NonZeroRange<T>
    where
        NonZeroRange<T>: CreateRange<Item = T>,
        T: Copy,
    {
        NonZeroRange::new_debug_checked(self.x, self.width)
    }

    pub fn into_rect_iter<R: CreateRange<Item = T>>(
        self,
        global_width: T::NonZero,
    ) -> RectIterator<T, R>
    where
        T: num_traits::Zero
            + Copy
            + Debug
            + PartialEq
            + std::ops::Mul<Output = T>
            + std::ops::Add<Output = T>
            + PartialOrd,
        T::NonZero: PartialOrd,
    {
        RectIterator::new(self.x, self.y, self.width, self.height, global_width)
    }
}
