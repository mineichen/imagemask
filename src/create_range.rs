use crate::{MetaRange, NonZeroRange, SignedNonZeroable};

pub trait CreateRange: Sized {
    type Item: SignedNonZeroable;
    type ListItem<TMeta>: From<(Self, TMeta)>;
    fn new_debug_checked(
        start: Self::Item,
        len: <Self::Item as SignedNonZeroable>::NonZero,
    ) -> Self;
    fn start(&self) -> Self::Item;
    fn end(&self) -> Self::Item;
}

impl<
    T: SignedNonZeroable
        + Copy
        + num_traits::One
        + std::ops::Sub<Output = T>
        + std::ops::Add<Output = T>,
> CreateRange for std::ops::RangeInclusive<T>
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

    fn start(&self) -> Self::Item {
        *std::ops::RangeInclusive::start(self)
    }
    fn end(&self) -> Self::Item {
        *std::ops::RangeInclusive::end(self) + T::one()
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

    fn start(&self) -> Self::Item {
        self.start
    }
    fn end(&self) -> Self::Item {
        self.end
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
    fn start(&self) -> Self::Item {
        self.start
    }
    fn end(&self) -> Self::Item {
        self.end
    }
}
