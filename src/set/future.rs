use std::fmt::Display;

use futures_core::Stream;

use crate::CreateRange;

use super::{Builder, SortedRanges};

impl<TIncluded, TExcluded> SortedRanges<TIncluded, TExcluded> {
    pub fn try_from_ordered_stream<TStream, T>(
        stream: TStream,
    ) -> TryFromOrderedStreamFuture<TStream, TIncluded, TExcluded>
    where
        TStream: Stream<Item = std::io::Result<T>>,
        T: CreateRange<Item: TryInto<u64, Error: Display>>,
        TIncluded: TryFrom<u64, Error: Display>,
        TExcluded: TryFrom<u64, Error: Display>,
    {
        TryFromOrderedStreamFuture {
            stream: stream,
            builder: None,
        }
    }
}
pin_project_lite::pin_project!(
    pub struct TryFromOrderedStreamFuture<S, TIncluded, TExcluded> {
        #[pin]
        stream: S,
        builder: Option<Builder<TIncluded, TExcluded>>,
    }
);
impl<S: Stream<Item = std::io::Result<T>>, T, TIncluded, TExcluded> std::future::Future
    for TryFromOrderedStreamFuture<S, TIncluded, TExcluded>
where
    TIncluded: TryFrom<u64, Error: Display>,
    TExcluded: TryFrom<u64, Error: Display>,
    T: CreateRange<Item: TryInto<u64, Error: Display>>,
{
    type Output = Result<SortedRanges<TIncluded, TExcluded>, String>;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let mut this = self.project();
        if this.builder.is_none() {
            let size_hint = this.stream.size_hint().0;
            match std::task::ready!(this.stream.as_mut().poll_next(cx)) {
                Some(Ok(first_range)) => match Builder::new(first_range, size_hint) {
                    Ok(x) => *this.builder = Some(x),
                    Err(e) => return std::task::Poll::Ready(Err(e)),
                },
                Some(Err(e)) => return std::task::Poll::Ready(Err(e.to_string())),
                None => return std::task::Poll::Ready(Err("Requires at least one item".into())),
            }
        };
        loop {
            match std::task::ready!(this.stream.as_mut().poll_next(cx)) {
                Some(Ok(x)) => {
                    let builder = this
                        .builder
                        .as_mut()
                        .expect("Created if non existend... Lifetime issue");
                    if let Err(e) = builder.add(x) {
                        return std::task::Poll::Ready(Err(e));
                    }
                }
                Some(Err(e)) => return std::task::Poll::Ready(Err(e.to_string())),
                None => return std::task::Poll::Ready(Ok(this.builder.take().unwrap().build())),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use testresult::TestResult;

    use super::*;

    #[tokio::test]
    async fn try_from_stream() -> TestResult {
        let ranges_array = [0u8..10, 16..20];
        let ranges = SortedRanges::<u64, u64>::try_from_ordered_stream(futures_util::stream::iter(
            ranges_array.iter().map(|x| Ok(x.clone())),
        ))
        .await?;
        assert_eq!(
            SortedRanges::<u64, u64>::try_from_ordered_iter(ranges_array)?,
            ranges
        );
        Ok(())
    }
}
