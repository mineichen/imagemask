use std::collections::VecDeque;

use crate::range::NonZeroRange;

use super::{DebugAssertSortedByIter, OrderedRangeItem};

///
/// Removes overlaps based on priority
/// Very long overlaps lead to
///
pub struct OrderedNonOverlappingRangeIter<TIter, TMeta> {
    iter: std::iter::Fuse<TIter>,
    // Overlaps are resolved at this point
    remainers: VecDeque<OrderedRangeItem<usize, TMeta>>,
    max_start: usize,
}
impl<TIter, TMeta: Clone> OrderedNonOverlappingRangeIter<TIter, TMeta>
where
    TIter: Iterator<Item = OrderedRangeItem<usize, TMeta>>,
{
    pub fn new(
        iter: TIter,
    ) -> OrderedNonOverlappingRangeIter<impl Iterator<Item = OrderedRangeItem<usize, TMeta>>, TMeta>
    {
        OrderedNonOverlappingRangeIter {
            iter: DebugAssertSortedByIter::new(iter, |x: &OrderedRangeItem<usize, TMeta>| {
                x.comparator()
            })
            .fuse(),
            remainers: Default::default(),
            max_start: 0,
        }
    }

    fn insert_remainer(
        ordered_non_overlapping: &mut VecDeque<OrderedRangeItem<usize, TMeta>>,
        mut new: OrderedRangeItem<usize, TMeta>,
    ) {
        let mut idx = ordered_non_overlapping.len();

        while idx > 0 {
            idx -= 1;
            let existing = &mut ordered_non_overlapping[idx];
            if existing.range.end <= new.range.start {
                // println!("No overlap anymore");
                ordered_non_overlapping.insert(idx + 1, new);
                return;
            }
            if existing.priority >= new.priority {
                // Never a rest before (existing.range.start is always < new.range.start)
                if let Ok(after_existing) =
                    NonZeroRange::try_from(existing.range.end..new.range.end)
                {
                    new.range = after_existing;
                    ordered_non_overlapping.insert(idx + 1, new);
                } else {
                }
                return;
            } else
            /* new has prio */
            {
                let maybe_after = NonZeroRange::try_from(new.range.end..existing.range.end);
                let maybe_before = NonZeroRange::try_from(existing.range.start..new.range.start);
                match (maybe_before, maybe_after) {
                    (Ok(before), Ok(after)) => {
                        //println!("E");
                        existing.range = before;
                        let after_value = OrderedRangeItem {
                            range: after,
                            priority: existing.priority,
                            meta: existing.meta.clone(),
                        };
                        ordered_non_overlapping.insert(idx + 1, new);
                        ordered_non_overlapping.insert(idx + 2, after_value);
                        return;
                    }
                    (Ok(before), Err(_)) => {
                        // println!("F");
                        existing.range = before;
                        ordered_non_overlapping.insert(idx + 1, new);
                        return;
                    }
                    (Err(_), Ok(after)) => {
                        // println!("G");
                        existing.range = after;
                    }
                    (Err(_), Err(_)) => {
                        ordered_non_overlapping.remove(idx);
                    }
                }
            }
        }
        ordered_non_overlapping.push_front(new);
    }
}

impl<TIter, TMeta: Clone> Iterator for OrderedNonOverlappingRangeIter<TIter, TMeta>
where
    TIter: Iterator<Item = OrderedRangeItem<usize, TMeta>>,
{
    type Item = OrderedRangeItem<usize, TMeta>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut first_end = if let Some(smallest) = self.remainers.front() {
            // There could be more than one with same start
            if smallest.range.end < self.max_start {
                return self.remainers.pop_front();
            } else {
                Some(smallest.range.end)
            }
        } else {
            None
        };

        for next in &mut self.iter {
            //println!("Pop next {:?}", next.range);
            self.max_start = next.range.start;
            let next_range_end = next.range.end;
            Self::insert_remainer(&mut self.remainers, next);

            //dbg!(&self.remainers.iter().map(|x| x.range).collect::<Vec<_>>());
            if *first_end.get_or_insert(next_range_end) < self.max_start {
                break;
            }
        }
        //println!("Release {:?}", self.remainers.front().map(|x| x.range));
        self.remainers.pop_front()
    }
}

#[cfg(test)]
mod tests {
    use std::u32;

    use itertools::Itertools;

    use crate::range::NonZeroRange;

    use super::*;

    #[test]
    fn priority_overlaps_before() {
        let iter = OrderedNonOverlappingRangeIter::new(
            [
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..10),
                    priority: 1,
                    meta: (),
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(5..15),
                    priority: 0,
                    meta: (),
                },
            ]
            .into_iter(),
        );
        assert_eq!(
            vec!(0..10, 10..15),
            iter.map(|x| x.range.into()).collect_vec()
        );
    }

    #[test]
    fn same_start_with_inner_high_prio() {
        let iter = OrderedNonOverlappingRangeIter::new(
            [
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..15),
                    priority: 0,
                    meta: 42,
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..10),
                    priority: 0,
                    meta: 84,
                },
            ]
            .into_iter(),
        );
        assert_eq!(
            vec!((0..15, 42)),
            iter.map(|x| (x.range.into(), x.meta)).collect_vec()
        );
    }

    #[test]
    fn ignore_same_priority_extending_first() {
        let iter = OrderedNonOverlappingRangeIter::new(
            [
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..10),
                    priority: 0,
                    meta: (),
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..20),
                    priority: 0,
                    meta: (),
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(1..15),
                    priority: 2,
                    meta: (),
                },
            ]
            .into_iter(),
        );
        assert_eq!(
            vec!(0..1, 1..15, 15..20),
            iter.map(|x| x.range.into()).collect_vec()
        );
    }
    #[test]
    fn ignore_same_priority_extending_first_same_range() {
        let iter = OrderedNonOverlappingRangeIter::new(
            [
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..10),
                    priority: 0,
                    meta: 10,
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..20),
                    priority: 0,
                    meta: 20,
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(10..20),
                    priority: 2,
                    meta: 30,
                },
            ]
            .into_iter(),
        );
        assert_eq!(
            vec!((0..10, 10), (10..20, 30)),
            iter.map(|x| (x.range.into(), x.meta)).collect_vec()
        );
    }
    #[test]
    fn ignore_low_priority_in_the_middle() {
        let iter = OrderedNonOverlappingRangeIter::new(
            [
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(0..10),
                    priority: 1,
                    meta: (),
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(1..15),
                    priority: 2,
                    meta: (),
                },
                OrderedRangeItem {
                    range: NonZeroRange::new_unchecked(1..15),
                    priority: 0,
                    meta: (),
                },
            ]
            .into_iter(),
        );
        assert_eq!(
            vec!(0..1, 1..15),
            iter.map(|x| x.range.into()).collect_vec()
        );
    }

    #[test]
    fn merge_non_overlapping_same_prio() {
        assert_eq!(
            vec![12..14, 16..17],
            OrderedNonOverlappingRangeIter::new(
                vec![
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(12..14),
                        meta: 1,
                        priority: u32::MAX,
                    },
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(16..17),
                        meta: 1,
                        priority: u32::MAX,
                    },
                ]
                .into_iter(),
            )
            .map(|x| std::ops::Range::from(x.range))
            .collect_vec()
        );
    }
    #[test]
    fn surrounding_same_prio() {
        assert_eq!(
            vec![12..18],
            OrderedNonOverlappingRangeIter::new(
                vec![
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(12..18),
                        meta: 1,
                        priority: u32::MAX,
                    },
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(13..17),
                        meta: 1,
                        priority: u32::MAX,
                    },
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(16..17),
                        meta: 1,
                        priority: u32::MAX,
                    },
                ]
                .into_iter(),
            )
            .map(|x| std::ops::Range::from(x.range))
            .collect_vec()
        );
    }

    #[test]
    fn surrounding_low_nostart_prio() {
        assert_eq!(
            vec![(12..16, 0), (16..17, 2), (17..18, 0)],
            OrderedNonOverlappingRangeIter::new(
                vec![
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(12..18),
                        meta: 1,
                        priority: 0
                    },
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(16..17),
                        meta: 1,
                        priority: 2,
                    },
                    OrderedRangeItem {
                        range: NonZeroRange::new_unchecked(16..17),
                        meta: 1,
                        priority: 1
                    },
                ]
                .into_iter(),
            )
            .map(|x| (std::ops::Range::from(x.range), x.priority))
            .collect_vec()
        );
    }
}
