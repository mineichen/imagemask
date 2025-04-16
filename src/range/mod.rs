///
/// Working with ranges or collections/iterators of ranges
///
mod assert_sorted_iter;
mod merge_ordered_iter;
mod non_zero;

use std::fmt::Debug;

pub use assert_sorted_iter::*;
pub use merge_ordered_iter::*;
pub use non_zero::*;

#[derive(Debug, Eq, PartialEq)]
pub struct OrderedRangeItem<TMeta> {
    pub range: NonZeroRange,
    pub meta: TMeta,
    pub priority: u32,
}

impl<TMeta> OrderedRangeItem<TMeta> {
    pub fn comparator(&self) -> (u32, u32) {
        (self.range.start, u32::MAX - self.priority)
    }
}
