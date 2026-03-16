use std::{
    future::Future,
    io,
    ops::RangeInclusive,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};

use futures_io::{AsyncRead, AsyncWrite};
use pin_project_lite::pin_project;

use crate::NonZeroRange;

const PROTOCOL_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    U64 = 0,
}

impl TryFrom<u8> for DataType {
    type Error = ProtocolError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(DataType::U64),
            _ => Err(ProtocolError::InvalidDataType(value)),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ProtocolError {
    #[error("Invalid protocol version: {0}")]
    InvalidVersion(u8),
    #[error("Invalid data type: {0}")]
    InvalidDataType(u8),
    #[error("Ranges are not disjoint")]
    NonDisjointRanges,
    #[error("IO error")]
    Io(#[source] Arc<io::Error>),
    #[error("Unexpected end of stream")]
    UnexpectedEof,
    #[error("Range start {start} is before previous end {last_end}")]
    OverlappingRange { start: u64, last_end: u64 },
}

impl From<io::Error> for ProtocolError {
    fn from(e: io::Error) -> Self {
        ProtocolError::Io(Arc::new(e))
    }
}

#[derive(Debug, Clone)]
struct Header {
    version: u8,
    included_type: DataType,
    excluded_type: DataType,
}

impl Header {
    const SIZE: usize = 3;
    fn new(included_type: DataType, excluded_type: DataType) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            excluded_type,
            included_type,
        }
    }
    fn to_bytes(&self) -> [u8; Self::SIZE] {
        [
            self.version,
            self.included_type as u8,
            self.excluded_type as u8,
        ]
    }
    fn from_bytes(bytes: [u8; Self::SIZE]) -> Result<Self, ProtocolError> {
        if bytes[0] != PROTOCOL_VERSION {
            return Err(ProtocolError::InvalidVersion(bytes[0]));
        }
        Ok(Self {
            version: bytes[0],
            included_type: DataType::try_from(bytes[1])?,
            excluded_type: DataType::try_from(bytes[2])?,
        })
    }
}

const U64_SIZE: usize = 8;

fn write_u64(buf: &mut [u8], val: u64) {
    buf[..U64_SIZE].copy_from_slice(&val.to_le_bytes());
}
fn read_u64(buf: &[u8]) -> u64 {
    u64::from_le_bytes(buf[..U64_SIZE].try_into().unwrap())
}

fn poll_write_all<W: AsyncWrite + ?Sized>(
    mut writer: Pin<&mut W>,
    cx: &mut Context<'_>,
    buf: &[u8],
    offset: &mut usize,
) -> Poll<Result<(), ProtocolError>> {
    while *offset < buf.len() {
        match ready!(writer.as_mut().poll_write(cx, &buf[*offset..]))? {
            0 => return Poll::Ready(Err(ProtocolError::UnexpectedEof)),
            n => *offset += n,
        }
    }
    Poll::Ready(Ok(()))
}

fn poll_read_exact<R: AsyncRead + ?Sized>(
    mut reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut [u8],
    offset: &mut usize,
) -> Poll<Result<(), ProtocolError>> {
    while *offset < buf.len() {
        match ready!(reader.as_mut().poll_read(cx, &mut buf[*offset..]))? {
            0 => return Poll::Ready(Err(ProtocolError::UnexpectedEof)),
            n => *offset += n,
        }
    }
    Poll::Ready(Ok(()))
}

pin_project! {
    pub struct AsyncRangeWriter<W, S> {
        #[pin] writer: W,
        #[pin] stream: S,
        state: WriterState,
        buf: [u8; U64_SIZE * 2],
        pos: usize,
        len: usize,
        last_end: u64,
    }
}

enum WriterState {
    Header,
    ReadRange,
    WriteBuf,
    Closing,
    Done,
}

impl<W, S> AsyncRangeWriter<W, S> {
    pub fn new(writer: W, stream: S) -> Self {
        Self {
            writer,
            stream,
            state: WriterState::Header,
            buf: [0; U64_SIZE * 2],
            pos: 0,
            len: Header::SIZE,
            last_end: 0,
        }
    }
}

impl<W: AsyncWrite, S: futures_core::Stream<Item = RangeInclusive<u64>>> Future
    for AsyncRangeWriter<W, S>
{
    type Output = Result<(), ProtocolError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match &mut this.state {
                WriterState::Header => {
                    let header = Header::new(DataType::U64, DataType::U64);
                    this.buf[..Header::SIZE].copy_from_slice(&header.to_bytes());
                    *this.state = WriterState::WriteBuf;
                }
                WriterState::ReadRange => match ready!(this.stream.as_mut().poll_next(cx)) {
                    Some(r) => {
                        let (s, e) = r.into_inner();
                        let (start, end) = (s, e);
                        if start < *this.last_end {
                            return Poll::Ready(Err(ProtocolError::OverlappingRange {
                                start,
                                last_end: *this.last_end,
                            }));
                        }
                        let len = end - start + 1;
                        let gap = start - *this.last_end;
                        write_u64(&mut this.buf[..], gap);
                        write_u64(&mut this.buf[U64_SIZE..], len);
                        *this.last_end = e + 1;
                        *this.len = this.buf.len();
                        *this.state = WriterState::WriteBuf;
                    }
                    None => *this.state = WriterState::Closing,
                },
                WriterState::WriteBuf => {
                    ready!(poll_write_all(
                        this.writer.as_mut(),
                        cx,
                        &this.buf[..*this.len],
                        this.pos
                    ))?;
                    *this.pos = 0;
                    *this.state = WriterState::ReadRange;
                }
                WriterState::Closing => {
                    ready!(this.writer.as_mut().poll_close(cx))?;
                    *this.state = WriterState::Done;
                }
                WriterState::Done => {
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }
}

pin_project! {
    pub struct AsyncRangeStream<R> {
        #[pin] reader: R,
        buf: [u8; U64_SIZE * 2],
        pos: usize,
        last_end: u64,
        header: Option<Header>,
    }
}

impl<R: AsyncRead + Unpin> AsyncRangeStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buf: [0; U64_SIZE * 2],
            pos: 0,
            last_end: 0,
            header: None,
        }
    }
}

impl<R: AsyncRead> futures_core::Stream for AsyncRangeStream<R> {
    type Item = Result<NonZeroRange<u64>, ProtocolError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if this.header.is_none() {
            ready!(poll_read_exact(
                this.reader.as_mut(),
                cx,
                &mut this.buf[..Header::SIZE],
                this.pos,
            ))?;

            *this.header = Some(Header::from_bytes([this.buf[0], this.buf[1], this.buf[2]])?);
            *this.pos = 0;
        }

        match ready!(poll_read_exact(
            this.reader.as_mut(),
            cx,
            this.buf,
            this.pos
        )) {
            Ok(()) => {
                let gap = read_u64(&this.buf[..]);
                let len = read_u64(&this.buf[U64_SIZE..]);
                if len == 0 {
                    return Poll::Ready(None);
                }
                let start = *this.last_end + gap;
                let end = start + len - 1;
                *this.last_end = end + 1;
                *this.pos = 0;
                Poll::Ready(Some(Ok(NonZeroRange::new(start..end + 1))))
            }
            Err(ProtocolError::UnexpectedEof) if *this.pos == 0 => Poll::Ready(None),
            Err(e) => Poll::Ready(Some(Err(e))),
        }
    }
}

#[cfg(test)]
mod tests {

    use futures_util::TryStreamExt;

    use super::*;

    fn test_ranges(n: usize) -> Vec<RangeInclusive<u64>> {
        (0..n)
            .map(|i| {
                let s = (i as u64) * 20;
                s..=s + 5 + (i as u64 % 10)
            })
            .collect()
    }

    fn make_header_bytes() -> Vec<u8> {
        Header::new(DataType::U64, DataType::U64)
            .to_bytes()
            .to_vec()
    }

    fn make_range_bytes(gap: u64, len: u64) -> Vec<u8> {
        let mut buf = vec![0u8; 16];
        write_u64(&mut buf[..8], gap);
        write_u64(&mut buf[8..], len);
        buf
    }

    #[tokio::test]
    async fn roundtrip() {
        let ranges = test_ranges(100);
        let expected: Vec<_> = ranges
            .iter()
            .map(|r| NonZeroRange::new(*r.start()..*r.end() + 1))
            .collect();
        let mut buf = Vec::new();
        let stream = futures_util::stream::iter(ranges.iter().cloned());
        let writer = AsyncRangeWriter::new(&mut buf, stream);
        writer.await.unwrap();
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert_eq!(expected, result);
    }

    #[tokio::test]
    async fn empty_error() {
        let reader = AsyncRangeStream::new(&[][..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::UnexpectedEof)));
    }

    #[tokio::test]
    async fn header_only_is_empty() {
        let buf = make_header_bytes();
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn overlapping_ranges_error() {
        let mut buf = Vec::new();
        let ranges = vec![10..=20u64, 15..=25];
        let writer = AsyncRangeWriter::new(&mut buf, futures_util::stream::iter(ranges));
        let result = writer.await;
        assert!(matches!(
            result,
            Err(ProtocolError::OverlappingRange { .. })
        ));
    }

    #[tokio::test]
    async fn out_of_order_ranges_error() {
        let mut buf = Vec::new();
        let ranges = vec![50..=60u64, 10..=20];
        let writer = AsyncRangeWriter::new(&mut buf, futures_util::stream::iter(ranges));
        let result = writer.await;
        assert!(matches!(
            result,
            Err(ProtocolError::OverlappingRange { .. })
        ));
    }

    #[tokio::test]
    async fn invalid_protocol_version() {
        let buf = vec![0x99, 0x00, 0x00];
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::InvalidVersion(0x99))));
    }

    #[tokio::test]
    async fn invalid_included_data_type() {
        let buf = vec![PROTOCOL_VERSION, 0x05, 0x00];
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::InvalidDataType(0x05))));
    }

    #[tokio::test]
    async fn invalid_excluded_data_type() {
        let buf = vec![PROTOCOL_VERSION, 0x00, 0xFF];
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::InvalidDataType(0xFF))));
    }

    #[tokio::test]
    async fn truncated_header_one_byte() {
        let buf = vec![PROTOCOL_VERSION];
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::UnexpectedEof)));
    }

    #[tokio::test]
    async fn truncated_header_two_bytes() {
        let buf = vec![PROTOCOL_VERSION, 0x00];
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::UnexpectedEof)));
    }

    #[tokio::test]
    async fn truncated_range_partial_gap() {
        let mut buf = make_header_bytes();
        buf.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::UnexpectedEof)));
    }

    #[tokio::test]
    async fn truncated_range_gap_complete_len_partial() {
        let mut buf = make_header_bytes();
        buf.extend_from_slice(&make_range_bytes(10, 100)[..12]);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::UnexpectedEof)));
    }

    #[tokio::test]
    async fn single_range_roundtrip() {
        let ranges = vec![100..=200u64];
        let mut buf = Vec::new();
        let writer = AsyncRangeWriter::new(&mut buf, futures_util::stream::iter(ranges));
        writer.await.unwrap();
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert_eq!(result, vec![NonZeroRange::new(100u64..201)]);
    }

    #[tokio::test]
    async fn adjacent_ranges_roundtrip() {
        let ranges = vec![10..=20u64, 21..=30];
        let mut buf = Vec::new();
        let writer = AsyncRangeWriter::new(&mut buf, futures_util::stream::iter(ranges));
        writer.await.unwrap();
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert_eq!(
            result,
            vec![NonZeroRange::new(10u64..21), NonZeroRange::new(21u64..31)]
        );
    }

    #[tokio::test]
    async fn len_zero_terminates_stream() {
        let mut buf = make_header_bytes();
        buf.extend_from_slice(&make_range_bytes(10, 100));
        buf.extend_from_slice(&make_range_bytes(5, 0));
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert_eq!(result, vec![NonZeroRange::new(10u64..110)]);
    }

    #[tokio::test]
    async fn io_error_on_read() {
        use std::io::{Error, ErrorKind};
        use std::pin::Pin;
        use std::task::{Context, Poll};

        struct FailingReader;

        impl AsyncRead for FailingReader {
            fn poll_read(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                _buf: &mut [u8],
            ) -> Poll<std::io::Result<usize>> {
                Poll::Ready(Err(Error::new(ErrorKind::Other, "test error")))
            }
        }

        let reader = AsyncRangeStream::new(FailingReader);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(result, Err(ProtocolError::Io(_))));
    }

    #[tokio::test]
    async fn writer_empty_stream() {
        let mut buf = Vec::new();
        let ranges: Vec<RangeInclusive<u64>> = vec![];
        let writer = AsyncRangeWriter::new(&mut buf, futures_util::stream::iter(ranges));
        writer.await.unwrap();
        assert_eq!(buf.len(), Header::SIZE);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn writer_io_error() {
        use std::io::{Error, ErrorKind};
        use std::pin::Pin;
        use std::task::{Context, Poll};

        struct FailingWriter;

        impl AsyncWrite for FailingWriter {
            fn poll_write(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
                _buf: &[u8],
            ) -> Poll<std::io::Result<usize>> {
                Poll::Ready(Err(Error::new(ErrorKind::Other, "write error")))
            }

            fn poll_flush(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<std::io::Result<()>> {
                Poll::Ready(Ok(()))
            }

            fn poll_close(
                self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<std::io::Result<()>> {
                Poll::Ready(Ok(()))
            }
        }

        let writer = FailingWriter;
        let ranges = vec![10..=20u64];
        let async_writer = AsyncRangeWriter::new(writer, futures_util::stream::iter(ranges));
        let result = async_writer.await;
        assert!(matches!(result, Err(ProtocolError::Io(_))));
    }
}
