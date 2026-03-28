use std::{fmt::Debug, iter::FusedIterator, ops::RangeInclusive};

/// Sanitize a SortedStarts into a disjoint iterator of ranges. Iteration stops when error is detected.
/// It will merge adjacent ranges but fail, if a item with a smaller start than the current accumulator.start is found, as this could mean, that the output would not be sorted.
///
/// The Example shows a edge-case, where unsorted inputs are processed successfully, as long as the accumulator.start is smaller than the next item.start
///
/// # Panics
/// Panics in drop if a error occured during iteration and SanitizeSortedDisjoint::into_result was not called (See `SanitizeSortedDisjointError` for more details)
/// If SanitizeSortedDisjoint::into_result is called after iteration, this function is panic-free and can be used with untrusted input
/// ```
/// let result = imask::SanitizeSortedDisjoint::new([1u8..=5, 6..=8, 5..=10, 20..=30]).collect::<Vec<_>>();
/// assert_eq!(result, vec![1..=10, 20..=30]);
/// ```
/// Error handling example:
/// ```
/// use imask::{SanitizeSortedDisjoint, SanitizeSortedDisjointError};
///
/// let mut iter = SanitizeSortedDisjoint::new([0u8..=1, 2u8..=0u8]);
/// let result = (&mut iter).collect::<Vec<_>>();
/// assert_eq!(vec![0u8..=1], result);
/// assert_eq!(Err(SanitizeSortedDisjointError::StartAfterEnd { start: 2, end: 0 }), iter.into_result());
/// ```
///
/// ```
/// use imask::{SanitizeSortedDisjoint, SanitizeSortedDisjointError};
///
/// let mut iter = SanitizeSortedDisjoint::new([10u8..=11, 9u8..=10u8]);
/// let result = (&mut iter).collect::<Vec<_>>();
/// assert_eq!(vec![10u8..=11], result);
/// assert_eq!(Err(SanitizeSortedDisjointError::SmallerStartYielded { start: 9, end: 10, last_start: 10 }), iter.into_result());
/// ```
///
pub struct SanitizeSortedDisjoint<I, T: Debug> {
    iter: I,
    state: SanitizeSortedDisjointState<T>,
}

#[derive(Debug, Default)]
enum SanitizeSortedDisjointState<T> {
    Pending(RangeInclusive<T>),
    Error(SanitizeSortedDisjointError<T>),
    #[default]
    Fresh,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SanitizeSortedDisjointError<T> {
    #[error("Input not sorted by start")]
    SmallerStartYielded { start: T, end: T, last_start: T },

    #[error("Start after end: {start} > {end}")]
    StartAfterEnd { start: T, end: T },
}

impl<I, T: Debug> SanitizeSortedDisjoint<I, T> {
    pub fn new(iter: impl IntoIterator<IntoIter = I>) -> Self {
        Self {
            iter: iter.into_iter(),
            state: Default::default(),
        }
    }

    pub fn into_result(mut self) -> Result<(), SanitizeSortedDisjointError<T>> {
        let mut state = Default::default();
        std::mem::swap(&mut state, &mut self.state);
        if let SanitizeSortedDisjointState::Error(e) = state {
            Err(e)
        } else {
            Ok(())
        }
    }
    pub fn check(mut self) -> Result<Self, SanitizeSortedDisjointError<T>> {
        let mut state = Default::default();
        std::mem::swap(&mut state, &mut self.state);
        if let SanitizeSortedDisjointState::Error(e) = state {
            Err(e)
        } else {
            self.state = state;
            Ok(self)
        }
    }
}

impl<I, T: Debug> Drop for SanitizeSortedDisjoint<I, T> {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        {
            let mut state = Default::default();
            std::mem::swap(&mut state, &mut self.state);
            match state {
                SanitizeSortedDisjointState::Error(e) if std::thread::panicking() => {
                    eprintln!("SanitizeSortedDisjoint: {e:?}")
                }
                SanitizeSortedDisjointState::Error(e) => panic!("SanitizeSortedDisjoint: {e:?}"),
                _ => {}
            }
        }
    }
}

impl<I, T: Debug + num_traits::Unsigned + Ord + Copy> Iterator for SanitizeSortedDisjoint<I, T>
where
    I: Iterator<Item = RangeInclusive<T>>,
{
    type Item = RangeInclusive<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut iter = (&mut self.iter).map(|x| {
            let (start, end) = (*x.start(), *x.end());
            if start < end {
                Ok(x)
            } else {
                Err(SanitizeSortedDisjointError::StartAfterEnd { start, end })
            }
        });
        let mut last = Default::default();
        std::mem::swap(&mut self.state, &mut last);
        let mut last = match last {
            SanitizeSortedDisjointState::Pending(range_inclusive) => range_inclusive,
            SanitizeSortedDisjointState::Error(sanitize_error) => {
                self.state = SanitizeSortedDisjointState::Error(sanitize_error);
                return None;
            }
            SanitizeSortedDisjointState::Fresh => match iter.next()? {
                Ok(x) => x,
                Err(e) => {
                    self.state = SanitizeSortedDisjointState::Error(e);
                    return None;
                }
            },
        };
        loop {
            match iter.next() {
                None => return Some(last),
                Some(Err(e)) => {
                    self.state = SanitizeSortedDisjointState::Error(e);
                    return Some(last);
                }
                Some(Ok(next)) => {
                    let (last_start, next_start) = (*last.start(), *next.start());
                    let (last_end, next_end) = (*last.end(), *next.end());
                    if last_start > next_start {
                        self.state = SanitizeSortedDisjointState::Error(
                            SanitizeSortedDisjointError::SmallerStartYielded {
                                start: next_start,
                                end: next_end,
                                last_start,
                            },
                        );
                        return Some(last);
                    }
                    if next_start > last_end + T::one() {
                        self.state = SanitizeSortedDisjointState::Pending(next);
                        return Some(last);
                    }
                    last = last_start..=last_end.max(next_end);
                }
            }
        }
    }
}

impl<I, T: Debug + num_traits::Unsigned + Ord + Copy> FusedIterator for SanitizeSortedDisjoint<I, T> where
    I: FusedIterator<Item = RangeInclusive<T>>
{
}

#[cfg(feature = "range-set-blaze-0_5")]
mod range_set_blaze_0_5_interop {

    use range_set_blaze_0_5::{Integer, SortedDisjoint, SortedStarts};

    use super::*;

    impl<I, T: Integer + num_traits::Unsigned> SortedStarts<T> for SanitizeSortedDisjoint<I, T> where
        I: FusedIterator<Item = RangeInclusive<T>>
    {
    }
    impl<I, T: Integer + num_traits::Unsigned> SortedDisjoint<T> for SanitizeSortedDisjoint<I, T> where
        I: FusedIterator<Item = RangeInclusive<T>>
    {
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[cfg(feature = "range-set-blaze-0_5")]
    mod range_set_blaze_0_5_interop {
        use range_set_blaze_0_5::CheckSortedDisjoint;

        use super::*;

        #[test]
        fn adjacent_ranges_are_merged_for_check_sorted_disjoint() {
            let merge = SanitizeSortedDisjoint::new([1u8..=5, 6..=10, 20..=30]);
            let result = CheckSortedDisjoint::new(merge).collect::<Vec<_>>();
            assert_eq!(result, vec![1..=10, 20..=30]);
        }

        #[test]
        fn with_check_sorted_disjoint_overlapping_same_start() {
            let merged = SanitizeSortedDisjoint::new([1u16..=10, 1..=5, 1..=15, 1..=12]);
            let result = CheckSortedDisjoint::new(merged).collect::<Vec<_>>();
            assert_eq!(result, vec![1..=15]);
        }
        #[test]
        fn reproduce_user_crash_case() {
            let source_ranges = vec![
                2365505_u64..=2365559_u64,
                2365651_u64..=2365701_u64,
                2366806_u64..=2367960_u64,
                2367961_u64..=2368095_u64,
                2368662_u64..=2369039_u64,
            ];

            let merged = SanitizeSortedDisjoint::new(source_ranges.into_iter());
            CheckSortedDisjoint::new(merged).for_each(|_| {});
        }
    }
    #[test]
    fn empty() {
        let result =
            SanitizeSortedDisjoint::new([] as [RangeInclusive<u64>; 0]).collect::<Vec<_>>();
        assert_eq!(result, vec![]);
    }

    #[test]
    #[should_panic(expected = "other_error")]
    fn panic_with_unhandled_iterator_error() {
        let mut iter = SanitizeSortedDisjoint::new([1u32..=10, 0..=10]);
        (&mut iter).for_each(|_| {});
        panic!("other_error");
    }
    #[test]
    #[should_panic(expected = "other_error")]
    fn inner_panick_doesnt_abort() {
        SanitizeSortedDisjoint::new([1u32..=10, 0..=10]).for_each(|_| {
            panic!("other_error");
        });
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "StartAfterEnd { start: 10, end: 9 }")
    )]
    fn range_with_end_bigger_start_after_initial() {
        assert_eq!(
            Some(0..=2),
            SanitizeSortedDisjoint::new([0u32..=2, 10..=9]).next()
        );
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "StartAfterEnd { start: 10, end: 9 }")
    )]
    #[cfg_attr(
        not(debug_assertions),
        should_panic(expected = "Panic after wrong item")
    )]
    fn range_with_end_bigger_start_single() {
        SanitizeSortedDisjoint::new([10u32..=9]).next();
        panic!("Panic after wrong item");
    }

    #[test]
    fn last_range_has_not_the_highest_end() {
        let result = SanitizeSortedDisjoint::new([0u32..=10, 1..=8]).collect::<Vec<_>>();
        assert_eq!(result, vec![0..=10]);
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "SmallerStartYielded { start: 1, end: 3, last_start: 5 }")
    )]
    fn out_of_order_panics() {
        assert_eq!(1, SanitizeSortedDisjoint::new([5u32..=7, 1..=3]).count());
    }

    // Allowed as this still causes a valid output
    // In contrast, `out_of_order_with_sooner_start_then_accumulator_start` cannot know if a smaller range was released already without tracking more variables
    #[test]
    fn out_of_order_after_merge_is_accepted() {
        assert_eq!(
            SanitizeSortedDisjoint::new([1u32..=5, 3..=7, 2..=103]).collect::<Vec<_>>(),
            vec![1..=103]
        );
    }

    #[test]
    fn out_of_order_with_same_start_then_accumulator_start() {
        assert_eq!(
            vec![1..=21],
            SanitizeSortedDisjoint::new([1u32..=5, 4..=20, 1..=21]).collect::<Vec<_>>()
        );
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "SmallerStartYielded { start: 0, end: 103, last_start: 1 }")
    )]
    fn out_of_order_with_sooner_start_then_accumulator_start() {
        assert_eq!(
            1,
            SanitizeSortedDisjoint::new([1u32..=5, 3..=7, 0..=103]).count()
        );
    }

    #[test]
    fn two_disjoint() {
        let result = SanitizeSortedDisjoint::new([(1u32..=3), (5..=7)]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=3, 5..=7]);
    }

    #[test]
    fn two_overlapping() {
        let result = SanitizeSortedDisjoint::new([1u8..=5, 3..=7]).collect::<Vec<_>>();
        assert_eq!(result, vec![(1..=7)]);
    }

    #[test]
    fn two_touching() {
        let result = SanitizeSortedDisjoint::new([1u8..=3, 4..=7]).collect::<Vec<_>>();
        assert_eq!(result, vec![(1..=7)]);
    }

    #[test]
    fn two_touching_adjacent() {
        let result = SanitizeSortedDisjoint::new([1u8..=3, 3..=7]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=7]);
    }

    #[test]
    fn second_contained_in_first() {
        let result = SanitizeSortedDisjoint::new([1u8..=10, 3..=5]).collect::<Vec<_>>();
        assert_eq!(result, vec![(1..=10)]);
    }

    #[test]
    fn three_merge_all() {
        let result = SanitizeSortedDisjoint::new([1u8..=3, 2..=5, 4..=7]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=7]);
    }

    #[test]
    fn three_partial_merge() {
        let result = SanitizeSortedDisjoint::new([1u8..=3, 5..=7, 6..=9]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=3, 5..=9]);
    }

    #[test]
    fn many_interleaved() {
        let result = SanitizeSortedDisjoint::new([1u8..=3, 2..=4, 3..=5]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=5]);
    }

    #[test]
    fn same_start_different_end() {
        let result = SanitizeSortedDisjoint::new([1u8..=3, 1..=5, 1..=7]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=7]);
    }

    #[test]
    fn same_range_multiple_times() {
        let result = SanitizeSortedDisjoint::new([1u8..=3, 1..=3, 1..=3]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=3]);
    }

    #[test]
    fn fused_iterator_returns_none_after_exhaustion() {
        let mut iter = SanitizeSortedDisjoint::new([1u8..=3]);
        assert_eq!(iter.next(), Some(1..=3));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn same_start_smaller_end_after_larger() {
        let result = SanitizeSortedDisjoint::new([1u16..=10, 1..=5, 1..=3]).collect::<Vec<_>>();
        assert_eq!(result, vec![1..=10]);
    }

    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "SmallerStartYielded { start: 1, end: 15, last_start: 20 }")
    )]
    fn same_start_varied_ends_interleaved_with_others_panics() {
        assert_eq!(
            vec![1u8..=10, 20..=30],
            SanitizeSortedDisjoint::new([1u8..=5, 1..=10, 20..=30, 1..=15]).collect::<Vec<_>>()
        );
    }
}
