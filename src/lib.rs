#![doc = include_str!("../README.md")]

///
/// Working with ranges or collections/iterators of ranges
///
mod assert_sorted_iter;
#[cfg(feature = "async-io")]
mod async_io;
mod create_range;
mod map;
mod non_zero;
mod set;
mod unchecked_cast;

pub use assert_sorted_iter::*;
#[cfg(feature = "async-io")]
pub use async_io::*;
pub use create_range::*;
pub use map::*;
pub use non_zero::*;
pub use set::*;
pub use unchecked_cast::*;

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
