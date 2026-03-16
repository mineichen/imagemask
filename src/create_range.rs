use crate::{MetaRange, NonZeroRange, SignedNonZeroable};

pub trait CreateRange: Sized {
    type Item: SignedNonZeroable;
    type ListItem<TMeta>: From<(Self, TMeta)>;
    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self;
}

impl<T: SignedNonZeroable + Copy + num_traits::One + std::ops::Sub<Output = T>> CreateRange
    for std::ops::RangeInclusive<T>
{
    type Item = T;
    type ListItem<TMeta> = (Self, TMeta);

    #[inline]
    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self {
        let end = start.add_nonzero(len) - T::one();
        start..=end
    }
}

impl<T: SignedNonZeroable + Copy> CreateRange for std::ops::Range<T> {
    type Item = T;
    type ListItem<TMeta> = (Self, TMeta);

    #[inline]
    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self {
        let end = start.add_nonzero(len);
        start..end
    }
}

impl<T: SignedNonZeroable + Copy + PartialEq> CreateRange for NonZeroRange<T> {
    type Item = T;
    type ListItem<TMeta> = MetaRange<Self, TMeta>;

    #[inline]
    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self {
        NonZeroRange::from_span(start, len)
    }
}
