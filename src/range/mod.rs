use std::fmt::Debug;

///
/// Working with ranges or collections/iterators of ranges
///
mod assert_sorted_iter;
mod merge_ordered_iter;
mod non_zero;
mod sorted_ranges;
mod sorted_ranges_map;

pub use assert_sorted_iter::*;
pub use merge_ordered_iter::*;
pub use non_zero::*;
pub use sorted_ranges::*;
pub use sorted_ranges_map::*;

#[derive(Debug, Eq, PartialEq)]
pub struct OrderedRangeItem<TMeta> {
    pub range: NonZeroRange<u32>,
    pub meta: TMeta,
    pub priority: u32,
}

impl<TMeta> OrderedRangeItem<TMeta> {
    pub fn comparator(&self) -> (u32, u32) {
        (self.range.start, u32::MAX - self.priority)
    }
}

pub trait CreateRange: Sized {
    type Item: SignedNonZeroable;
    type ListItem<TMeta>: From<(Self, TMeta)>;
    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self;
}

impl<T: SignedNonZeroable + Copy + num_traits::One + std::ops::Sub<T, Output = T>> CreateRange
    for std::ops::RangeInclusive<T>
{
    type Item = T;
    type ListItem<TMeta> = (Self, TMeta);

    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self {
        let end = start.strict_add_nonzero(len) - T::one();
        start..=end
    }
}

impl<T: SignedNonZeroable + Copy> CreateRange for std::ops::Range<T> {
    type Item = T;
    type ListItem<TMeta> = (Self, TMeta);

    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self {
        let end = start.strict_add_nonzero(len);
        start..end
    }
}

impl<T: SignedNonZeroable + Copy + Debug + PartialEq> CreateRange for NonZeroRange<T> {
    type Item = T;
    type ListItem<TMeta> = MetaRange<Self, TMeta>;

    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self {
        NonZeroRange::from_span(start, len)
    }
}
