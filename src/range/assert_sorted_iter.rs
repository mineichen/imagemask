use std::fmt::Debug;

pub struct DebugAssertSortedByIter<T, TFn, TOrd>(T, Option<TOrd>, TFn);

impl<TIter, TFn, TOrd> DebugAssertSortedByIter<TIter, TFn, TOrd> {
    pub fn new(iter: TIter, func: TFn) -> Self {
        Self(iter, None, func)
    }
}

impl<T: Iterator, TFn: Fn(&T::Item) -> TOrd, TOrd: Ord + Eq + Debug> Iterator
    for DebugAssertSortedByIter<T, TFn, TOrd>
{
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let value = self.0.next()?;
        #[cfg(debug_assertions)]
        {
            let ord_value = (self.2)(&value);
            if let Some(ord_last) = self.1.take() {
                assert!(ord_value >= ord_last, "{:?}>={:?}", ord_value, ord_last);
            }
            self.1 = Some(ord_value);
        }

        Some(value)
    }
}
