use super::{MetaRange, NonEmptyOrderedRanges, NonZeroRange};

pub struct InplaceFlatMapper<'a, TIncluded, TExcluded, TMeta> {
    ranges: &'a mut NonEmptyOrderedRanges<TIncluded, TExcluded, Vec<TMeta>>,
    read_pos: usize,
    write_pos: usize,
    cache: Vec<MetaRange<TMeta>>,
    cache_pos: usize,
    last_written_end: u64,
}

impl<TIncluded, TExcluded, TMeta> NonEmptyOrderedRanges<TIncluded, TExcluded, Vec<TMeta>>
where
    TIncluded: Copy + TryFrom<u64> + Into<u64>,
    TExcluded: Copy + TryFrom<u64> + Into<u64>,
    TMeta: Clone,
{
    pub fn flat_map_inplace<TMap>(&mut self, handler: TMap)
    where
        TMap: for<'b> FnMut(
            MetaRange<TMeta>,
            &mut InplaceFlatMapper<'b, TIncluded, TExcluded, TMeta>,
        ),
    {
        let mut mapper = InplaceFlatMapper::new(self);
        mapper.map(handler);
    }
}

impl<'a, TIncluded, TExcluded, TMeta> InplaceFlatMapper<'a, TIncluded, TExcluded, TMeta>
where
    TIncluded: Copy + TryFrom<u64> + Into<u64>,
    TExcluded: Copy + TryFrom<u64> + Into<u64>,
    TMeta: Clone,
{
    fn new(ranges: &'a mut NonEmptyOrderedRanges<TIncluded, TExcluded, Vec<TMeta>>) -> Self {
        let initial_offset = ranges.initial_offset;
        Self {
            ranges,
            read_pos: 0,
            write_pos: 0,
            cache: Default::default(),
            cache_pos: 0,
            last_written_end: initial_offset,
        }
    }

    fn map(
        &mut self,
        mut handler: impl for<'b> FnMut(
            MetaRange<TMeta>,
            &mut InplaceFlatMapper<'b, TIncluded, TExcluded, TMeta>,
        ),
    ) {
        let mut current_offset = self.ranges.initial_offset;

        while self.read_pos < self.ranges.included.len() {
            let item = self.read_next_from_main(&mut current_offset);
            handler(item, self);
            self.drain_cache(&mut handler);
        }
        self.truncate_to_write_pos();
    }

    fn read_next_from_main(&mut self, current_offset: &mut u64) -> MetaRange<TMeta> {
        let include = self.ranges.included[self.read_pos];
        let meta = self.ranges.meta[self.read_pos].clone();
        let range_start = *current_offset;
        let range_end = *current_offset + include.into();
        let range = unsafe { NonZeroRange::new_unchecked(range_start..range_end) };

        *current_offset = range_end;
        if self.read_pos < self.ranges.excluded.len() {
            *current_offset += self.ranges.excluded[self.read_pos].into();
        }

        self.read_pos += 1;

        MetaRange { range, meta }
    }

    fn drain_cache(
        &mut self,
        handler: &mut impl for<'b> FnMut(
            MetaRange<TMeta>,
            &mut InplaceFlatMapper<'b, TIncluded, TExcluded, TMeta>,
        ),
    ) {
        while self.cache_pos < self.cache.len() {
            let item = self.cache[self.cache_pos].clone();
            self.cache_pos += 1;
            handler(item, self);
        }
    }

    fn truncate_to_write_pos(&mut self) {
        self.ranges.included.truncate(self.write_pos);
        self.ranges
            .excluded
            .truncate(self.write_pos.saturating_sub(1));
        self.ranges.meta.truncate(self.write_pos);
        if self.write_pos == 0 {
            self.ranges.initial_offset = 0;
        }
    }

    pub fn insert(&mut self, item: MetaRange<TMeta>) {
        let gap = item.range.start.saturating_sub(self.last_written_end);
        let include_len = item.range.end - item.range.start;

        if self.read_pos == self.write_pos && self.read_pos < self.ranges.included.len() {
            self.cache_remaining_items();
        }

        if self.write_pos > 0 {
            if let Ok(gap_val) = TExcluded::try_from(gap) {
                if self.write_pos - 1 < self.ranges.excluded.len() {
                    self.ranges.excluded[self.write_pos - 1] = gap_val;
                } else {
                    self.ranges.excluded.push(gap_val);
                }
            }
        } else {
            self.ranges.initial_offset = item.range.start;
        }

        if let Ok(include_val) = TIncluded::try_from(include_len) {
            if self.write_pos < self.ranges.included.len() {
                self.ranges.included[self.write_pos] = include_val;
                self.ranges.meta[self.write_pos] = item.meta;
            } else {
                self.ranges.included.push(include_val);
                self.ranges.meta.push(item.meta);
                self.read_pos += 1;
            }
        }

        self.write_pos += 1;
        self.last_written_end = item.range.end;
    }

    fn cache_remaining_items(&mut self) {
        let remaining = self.ranges.included.len() - self.read_pos;
        let to_cache = BATCH_TO_CACHE.min(remaining);

        if self.cache.len() > self.cache_pos * 8 {
            self.cache.drain(0..self.cache_pos);
            self.cache_pos = 0;
        }

        let mut offset = self.ranges.initial_offset;
        for i in 0..self.read_pos {
            offset += self.ranges.included[i].into();
            if i < self.ranges.excluded.len() {
                offset += self.ranges.excluded[i].into();
            }
        }

        for i in 0..to_cache {
            let idx = self.read_pos + i;
            let range_start = offset;
            let range_end = offset + self.ranges.included[idx].into();
            let range = unsafe { NonZeroRange::new_unchecked(range_start..range_end) };

            self.cache.push(MetaRange {
                range,
                meta: self.ranges.meta[idx].clone(),
            });

            offset = range_end;
            if idx < self.ranges.excluded.len() {
                offset += self.ranges.excluded[idx].into();
            }
        }
        self.read_pos += to_cache;
    }
}

const BATCH_TO_CACHE: usize = 10;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ranges() -> NonEmptyOrderedRanges<u64, u64, Vec<String>> {
        NonEmptyOrderedRanges::try_from_ordered_iter([
            (0u32..5, "a".to_string()),
            (20..25, "b".to_string()),
            (40..45, "c".to_string()),
        ])
        .unwrap()
    }

    #[test]
    fn duplicate_each_range() {
        let mut ranges = make_ranges();
        ranges.flat_map_inplace(|item, inserter| {
            let mut with_offset = item.clone_with_offset(10);
            with_offset.meta = with_offset.meta.clone() + with_offset.meta.as_str();
            inserter.insert(item);
            inserter.insert(with_offset);
        });
        assert_eq!(
            ranges.into_iter().collect::<Vec<_>>(),
            vec![
                MetaRange {
                    range: NonZeroRange::new(0..5),
                    meta: "a".to_string()
                },
                MetaRange {
                    range: NonZeroRange::new(10..15),
                    meta: "aa".to_string()
                },
                MetaRange {
                    range: NonZeroRange::new(20..25),
                    meta: "b".to_string()
                },
                MetaRange {
                    range: NonZeroRange::new(30..35),
                    meta: "bb".to_string()
                },
                MetaRange {
                    range: NonZeroRange::new(40..45),
                    meta: "c".to_string()
                },
                MetaRange {
                    range: NonZeroRange::new(50..55),
                    meta: "cc".to_string()
                },
            ]
        );
    }

    #[test]
    fn filter_every_second() {
        let mut ranges = make_ranges();
        let mut count = 0;
        ranges.flat_map_inplace(|item, inserter| {
            count += 1;
            if count % 2 == 0 {
                inserter.insert(MetaRange {
                    range: item.range,
                    meta: item.meta.clone(),
                });
            }
        });

        assert_eq!(
            ranges.into_iter().collect::<Vec<_>>(),
            vec![MetaRange {
                range: NonZeroRange::new(20..25),
                meta: "b".to_string()
            },]
        );
    }

    #[test]
    fn filter_all() {
        let mut ranges = make_ranges();
        ranges.flat_map_inplace(|_item, _inserter| {});

        assert_eq!(0, ranges.included.len());
    }

    #[test]
    fn assert_low_cache_capacity() {
        let mut ranges = NonEmptyOrderedRanges::<u64, u64, Vec<String>>::try_from_ordered_iter(
            (0..100u32).map(|i| (i * 10..i * 10 + 4, i.to_string())),
        )
        .unwrap();

        let cache_capacity = {
            let mut mapper = InplaceFlatMapper::new(&mut ranges);
            mapper.cache.reserve_exact(20);
            let mut count = 0u32;

            mapper.map(|item, inserter| {
                if count % 2 == 0 {
                    let mut with_offset = item.clone_with_offset(5);
                    with_offset.meta += ".2";
                    inserter.insert(item);
                    inserter.insert(with_offset);
                }
                count += 1;
            });
            mapper.cache.capacity()
        };

        assert_eq!(100, ranges.included.len());
        assert!(
            cache_capacity == 20,
            "Expected small cache: {}",
            cache_capacity
        );
    }
}
