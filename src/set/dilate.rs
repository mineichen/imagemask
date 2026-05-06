use std::fmt::Debug;
use std::iter::FusedIterator;
use std::ops::Add;

use num_traits::SaturatingSub;

use crate::{CreateRange, SanitizeSortedDisjoint};

pub struct DilateIter<TIter>
where
    TIter: Iterator<
        Item: CreateRange<
            Item: Debug
                      + Add<Output = <TIter::Item as CreateRange>::Item>
                      + SaturatingSub<Output = <TIter::Item as CreateRange>::Item>
                      + Copy,
        >,
    >,
{
    // parent: UnionIter<<TIter::Item as CreateRange>::Item, DilateXIter<TIter>>,
    //
    parent: SanitizeSortedDisjoint<DilateXIter<TIter>>,
}

impl<TIter> Iterator for DilateIter<TIter>
where
    TIter: Iterator<
        Item: CreateRange<
            Item: Add<Output = <TIter::Item as CreateRange>::Item>
                      + SaturatingSub<Output = <TIter::Item as CreateRange>::Item>
                      + Copy
                      + Debug,
        >,
    >,
    SanitizeSortedDisjoint<DilateXIter<TIter>>: Iterator<Item = TIter::Item>,
{
    type Item = TIter::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.parent.next()
    }
}

struct DilateXIter<TIter: Iterator<Item: CreateRange>> {
    parent: TIter,
    offset: <TIter::Item as CreateRange>::Item,
}

impl<TIter> Iterator for DilateXIter<TIter>
where
    TIter: Iterator<
        Item: CreateRange<
            Item: Add<Output = <TIter::Item as CreateRange>::Item>
                      + SaturatingSub<Output = <TIter::Item as CreateRange>::Item>
                      + Copy,
        >,
    >,
{
    type Item = TIter::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.parent.next()?;
        let start = item.start();
        let end = item.end();

        Some(TIter::Item::new_debug_checked_zeroable(
            start.saturating_sub(&self.offset),
            end + self.offset,
        ))
    }
}

impl<TIter> FusedIterator for DilateXIter<TIter>
where
    TIter: FusedIterator<Item: CreateRange>,
    DilateXIter<TIter>: Iterator,
{
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use crate::{ImaskSet, Rect, SortedRanges};

    use super::*;
    const NONZERO_80: NonZeroU32 = NonZeroU32::new(80).unwrap();

    #[test]
    fn dilate_2x() {
        let top = 5u32 * 80 + 50..5 * 80 + 52;
        let bottom = 6 * 80 + 50..6 * 80 + 52;
        let data = [top, bottom].with_roi(Rect::new(0, 10, NONZERO_80, NONZERO_80));
        let data_dilate = DilateIter {
            parent: SanitizeSortedDisjoint::new(DilateXIter {
                offset: 2,
                parent: data.into_iter(),
            }),
        }
        .collect::<Vec<_>>();
        let expected = (0..6)
            .map(|offset| (3 + offset) * 80 + 48..(3 + offset) * 80 + 54)
            .collect::<Vec<_>>();
        assert_eq!(data_dilate, expected);
    }
}
