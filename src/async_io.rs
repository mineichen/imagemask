use std::{
    future::Future,
    io::{self, ErrorKind},
    num::NonZeroU32,
    ops::{RangeInclusive, Sub},
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, ready},
};

use futures_io::{AsyncRead, AsyncWrite};
use pin_project_lite::pin_project;

use crate::{CreateRange, NonZeroRange};

const U32_SIZE: usize = std::mem::size_of::<u32>();
const U64_SIZE: usize = std::mem::size_of::<u64>();
const PROTOCOL_VERSION: u8 = 1;
const HEADER_SIZE: usize = 3 + U32_SIZE * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DataType {
    U64 = 0,
}

impl TryFrom<u8> for DataType {
    type Error = ReadProtocolError;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(DataType::U64),
            _ => Err(ReadProtocolError::UnsupportedDataType(value)),
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum WriteProtocolError {
    #[error(transparent)]
    RangeOrder(#[from] RangeOrderError),
    #[error(transparent)]
    Io(Arc<io::Error>),
}

impl From<std::io::Error> for WriteProtocolError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(Arc::new(value))
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ReadProtocolError {
    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
    #[error("Unsupported data type: {0}")]
    UnsupportedDataType(u8),
    #[error("Ranges are not disjoint")]
    NonDisjointRanges,
    #[error(transparent)]
    UnexpectedZero(#[from] UnexpectedZeroError),
    #[error(transparent)]
    Io(Arc<io::Error>),
    #[error(transparent)]
    RangeOrder(#[from] RangeOrderError),
}

impl From<io::Error> for ReadProtocolError {
    fn from(e: io::Error) -> Self {
        ReadProtocolError::Io(Arc::new(e))
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Unexpected zero")]
pub struct UnexpectedZeroError {
    buffer_pos: usize,
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("Range start {start} is before previous end {last_end}")]
pub struct RangeOrderError {
    start: u64,
    last_end: u64,
}

#[derive(Debug, Clone)]
struct Header {
    version: u8,
    included_type: DataType,
    excluded_type: DataType,
    offset_x: u32,
    offset_y: u32,
    width: NonZeroU32,
    height: NonZeroU32,
}

impl Header {
    fn new(
        included_type: DataType,
        excluded_type: DataType,
        offset_x: u32,
        offset_y: u32,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            excluded_type,
            included_type,
            offset_x,
            offset_y,
            width,
            height,
        }
    }
    fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0] = self.version;
        buf[1] = self.included_type as u8;
        buf[2] = self.excluded_type as u8;
        write_u32(&mut buf[3..], self.offset_x);
        write_u32(&mut buf[3 + U32_SIZE..], self.offset_y);
        write_u32(&mut buf[3 + U32_SIZE * 2..], self.width.get());
        write_u32(&mut buf[3 + U32_SIZE * 3..], self.height.get());
        buf
    }
    fn from_bytes(bytes: &[u8; HEADER_SIZE]) -> Result<Self, ReadProtocolError> {
        if bytes[0] != PROTOCOL_VERSION {
            return Err(ReadProtocolError::UnsupportedVersion(bytes[0]));
        }
        let width_pos = 3 + U32_SIZE * 2;
        let height_pos = 3 + U32_SIZE * 3;
        Ok(Self {
            version: bytes[0],
            included_type: DataType::try_from(bytes[1])?,
            excluded_type: DataType::try_from(bytes[2])?,
            offset_x: read_u32(&bytes[3..]),
            offset_y: read_u32(&bytes[3 + U32_SIZE..]),
            width: read_u32(&bytes[width_pos..])
                .try_into()
                .map_err(|_| UnexpectedZeroError {
                    buffer_pos: width_pos,
                })?,
            height: read_u32(&bytes[height_pos..])
                .try_into()
                .map_err(|_| UnexpectedZeroError {
                    buffer_pos: height_pos,
                })?,
        })
    }
}

fn write_u32(buf: &mut [u8], val: u32) {
    buf[..U32_SIZE].copy_from_slice(&val.to_le_bytes());
}
fn read_u32(buf: &[u8]) -> u32 {
    u32::from_le_bytes(buf[..U32_SIZE].try_into().unwrap())
}
fn write_u64(buf: &mut [u8], val: u64) {
    buf[..8].copy_from_slice(&val.to_le_bytes());
}
fn read_u64(buf: &[u8]) -> u64 {
    u64::from_le_bytes(buf[..8].try_into().unwrap())
}

fn unexpeced_eof() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Unexpected Eof")
}

fn poll_write_all<W: AsyncWrite + ?Sized>(
    mut writer: Pin<&mut W>,
    cx: &mut Context<'_>,
    buf: &[u8],
    offset: &mut usize,
) -> Poll<Result<(), std::io::Error>> {
    while *offset < buf.len() {
        match ready!(writer.as_mut().poll_write(cx, &buf[*offset..]))? {
            0 => {
                return Poll::Ready(Err(unexpeced_eof().into()));
            }
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
) -> Poll<Result<(), std::io::Error>> {
    while *offset < buf.len() {
        match ready!(reader.as_mut().poll_read(cx, &mut buf[*offset..]))? {
            0 => return Poll::Ready(Err(unexpeced_eof())),
            n => *offset += n,
        }
    }
    Poll::Ready(Ok(()))
}

// pin_project! {
//     pub struct AsyncRangeWriter<W, S> where S: Stream<Item: CreateRange> {
//         #[pin] writer: W,
//         #[pin] stream: S,
//         state: WriterState,
//         buf: [u8; HEADER_SIZE],
//         pos: usize,
//         len: usize,
//         last_end: <S::Item as CreateRange>::Item,
//         offset_x: <S::Item as CreateRange>::Item,
//         offset_y: <S::Item as CreateRange>::Item,
//         width: <<S::Item as CreateRange>::Item as SignedNonZeroable>::NonZero,
//         height: <<S::Item as CreateRange>::Item as SignedNonZeroable>::NonZero,
//     }
// }
pin_project! {
    pub struct AsyncRangeWriter<W, S> {
        #[pin] writer: W,
        #[pin] stream: S,
        state: WriterState,
        buf: [u8; HEADER_SIZE],
        pos: usize,
        len: usize,
        last_end: u64,
        pending_range: Option<(u64, u64, u64, u64)>,
        offset_x: u32,
        offset_y: u32,
        width: NonZeroU32,
        height: NonZeroU32,
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
    pub fn new(
        writer: W,
        stream: S,
        offset_x: u32,
        offset_y: u32,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Self {
        Self {
            writer,
            stream,
            state: WriterState::Header,
            buf: [0; HEADER_SIZE],
            pos: 0,
            len: 0,
            last_end: 0,
            pending_range: None,
            offset_x,
            offset_y,
            width,
            height,
        }
    }
}

impl<W, S> Future for AsyncRangeWriter<W, S>
where
    W: AsyncWrite,
    S: futures_core::Stream<
            Item: CreateRange<Item: Sub<Output = <S::Item as CreateRange>::Item> + Into<u64>>,
        >,
{
    type Output = Result<(), WriteProtocolError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        loop {
            match &mut this.state {
                WriterState::Header => {
                    let header = Header::new(
                        DataType::U64,
                        DataType::U64,
                        *this.offset_x,
                        *this.offset_y,
                        *this.width,
                        *this.height,
                    );
                    this.buf[..HEADER_SIZE].copy_from_slice(&header.to_bytes());
                    *this.len = HEADER_SIZE;
                    *this.state = WriterState::WriteBuf;
                }
                WriterState::ReadRange => match ready!(this.stream.as_mut().poll_next(cx)) {
                    Some(r) => {
                        let (global_start, global_end) = (r.start().into(), r.end().into());
                        let flat_offset = (u64::from(this.width.get()))
                            .wrapping_mul(u64::from(*this.offset_y))
                            .wrapping_add(u64::from(*this.offset_x));
                        let start = global_start - flat_offset;
                        let end = global_end - flat_offset;
                        let len = end - start;
                        let width = u64::from(this.width.get());
                        let ox = u64::from(*this.offset_x);
                        if let Some((pending_start, pending_len, pending_actual_end, gap_base)) =
                            *this.pending_range
                        {
                            if start < pending_actual_end {
                                return Poll::Ready(Err(RangeOrderError {
                                    start,
                                    last_end: pending_actual_end,
                                }
                                .into()));
                            }
                            let pending_ends_at_line_end =
                                ox > 0 && (pending_actual_end + ox) % width == 0;
                            let next_starts_at_line_start = (start + ox) % width == ox;
                            if pending_ends_at_line_end
                                && next_starts_at_line_start
                                && start == pending_actual_end + ox
                            {
                                *this.pending_range =
                                    Some((pending_start, pending_len + len, end, gap_base));
                                *this.last_end = end;
                                *this.state = WriterState::ReadRange;
                            } else {
                                let gap = pending_start - gap_base;
                                write_u64(&mut this.buf[..], gap);
                                write_u64(&mut this.buf[U64_SIZE..], pending_len);
                                *this.pending_range = Some((start, len, end, *this.last_end));
                                *this.last_end = end;
                                *this.len = U64_SIZE * 2;
                                *this.state = WriterState::WriteBuf;
                            }
                        } else {
                            if start < *this.last_end {
                                return Poll::Ready(Err(RangeOrderError {
                                    start,
                                    last_end: *this.last_end,
                                }
                                .into()));
                            }
                            *this.pending_range = Some((start, len, end, *this.last_end));
                            *this.last_end = end;
                            *this.state = WriterState::ReadRange;
                        }
                    }
                    None => {
                        if let Some((pending_start, pending_len, _pending_actual_end, gap_base)) =
                            *this.pending_range
                        {
                            let gap = pending_start - gap_base;
                            write_u64(&mut this.buf[..], gap);
                            write_u64(&mut this.buf[U64_SIZE..], pending_len);
                            *this.pending_range = None;
                            *this.len = U64_SIZE * 2;
                            *this.state = WriterState::WriteBuf;
                        } else {
                            *this.state = WriterState::Closing;
                        }
                    }
                },
                WriterState::WriteBuf => {
                    ready!(poll_write_all(
                        this.writer.as_mut(),
                        cx,
                        &this.buf[..*this.len],
                        this.pos
                    ))?;
                    *this.pos = 0;
                    *this.len = U64_SIZE * 2;
                    *this.state = WriterState::ReadRange;
                }
                WriterState::Closing => {
                    ready!(this.writer.as_mut().poll_close(cx))?;
                    if *this.last_end == 0 {
                        return Poll::Ready(Err(std::io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Expected at least 1 range",
                        )
                        .into()));
                    }
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
        buf: [u8; HEADER_SIZE],
        pos: usize,
        last_end: u64,
        header: Option<Header>,
        offset: u64,
        local: bool,
        pending_local_start: u64,
        pending_local_len: u64,
        crossed_line_gaps: u64,
    }
}

impl<R: AsyncRead + Unpin> AsyncRangeStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buf: [0; HEADER_SIZE],
            pos: 0,
            last_end: 0,
            header: None,
            offset: 0,
            local: false,
            pending_local_start: 0,
            pending_local_len: 0,
            crossed_line_gaps: 0,
        }
    }

    pub fn into_iter_local(self) -> Self {
        Self {
            local: true,
            ..self
        }
    }
}

impl<R: AsyncRead> futures_core::Stream for AsyncRangeStream<R> {
    type Item = Result<NonZeroRange<u64>, ReadProtocolError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        if this.header.is_none() {
            ready!(poll_read_exact(
                this.reader.as_mut(),
                cx,
                &mut this.buf[..HEADER_SIZE],
                this.pos,
            ))?;

            let header = Header::from_bytes(this.buf)?;
            *this.offset = u64::from(header.width.get())
                .wrapping_mul(u64::from(header.offset_y))
                .wrapping_add(u64::from(header.offset_x));
            println!("Read HEADER: {header:?}");
            *this.header = Some(header);
            *this.pos = 0;
        }

        loop {
            if *this.pending_local_len > 0 {
                let header = this.header.as_ref().unwrap();
                let width = u64::from(header.width.get());
                let ox = u64::from(header.offset_x);
                let ls = *this.pending_local_start;
                let local_end = ls + *this.pending_local_len;
                let global_start = ls + *this.offset;
                if ox == 0 {
                    *this.pending_local_len = 0;
                    let chunk = local_end - ls;
                    let ge = global_start + chunk - 1;
                    return Poll::Ready(Some(Ok(NonZeroRange::new(global_start..ge + 1))));
                }
                let global_start_line = (global_start + ox) / width;
                let local_line_end = (global_start_line + 1) * width - *this.offset;
                if local_line_end >= local_end {
                    *this.pending_local_len = 0;
                    let chunk = local_end - ls;
                    let ge = global_start + chunk - 1;
                    return Poll::Ready(Some(Ok(NonZeroRange::new(global_start..ge + 1))));
                } else {
                    let chunk = local_line_end - ls;
                    *this.pending_local_start = local_line_end + ox;
                    *this.pending_local_len -= chunk;
                    *this.crossed_line_gaps += 1;
                    let ge = global_start + chunk - 1;
                    return Poll::Ready(Some(Ok(NonZeroRange::new(global_start..ge + 1))));
                }
            }

            match ready!(poll_read_exact(
                this.reader.as_mut(),
                cx,
                &mut this.buf[..U64_SIZE * 2],
                this.pos
            )) {
                Ok(()) => {
                    let gap = read_u64(&this.buf[..]);
                    let len = read_u64(&this.buf[U64_SIZE..]);
                    println!("Read gap {gap}, len {len}, {:?}", &this.buf[..U64_SIZE * 2]);
                    if len == 0 {
                        return Poll::Ready(None);
                    }
                    let start = *this.last_end + gap;
                    let end = start + len;
                    *this.last_end = end;
                    *this.pos = 0;
                    if *this.local {
                        return Poll::Ready(Some(Ok(NonZeroRange::new(start..end))));
                    } else {
                        let header = this.header.as_ref().unwrap();
                        let width = u64::from(header.width.get());
                        let ox = u64::from(header.offset_x);
                        let global_start = start + *this.offset;
                        if ox == 0 {
                            let ge = global_start + len - 1;
                            return Poll::Ready(Some(Ok(NonZeroRange::new(global_start..ge + 1))));
                        }
                        let global_start_line = (global_start + ox) / width;
                        let local_line_end = (global_start_line + 1) * width - *this.offset;
                        if local_line_end >= end {
                            let ge = global_start + len - 1;
                            return Poll::Ready(Some(Ok(NonZeroRange::new(global_start..ge + 1))));
                        } else {
                            let chunk = local_line_end - start;
                            *this.pending_local_start = local_line_end + ox;
                            *this.pending_local_len = end - local_line_end;
                            *this.crossed_line_gaps = 1;
                            let ge = global_start + chunk - 1;
                            return Poll::Ready(Some(Ok(NonZeroRange::new(global_start..ge + 1))));
                        }
                    }
                }
                Err(e) if *this.pos == 0 && e.kind() == ErrorKind::UnexpectedEof => {
                    return Poll::Ready(None);
                }
                Err(e) => return Poll::Ready(Some(Err(e.into()))),
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::io::ErrorKind;

    use futures_util::{StreamExt, TryStreamExt};

    use super::*;

    const NONZERO_1000: NonZeroU32 = NonZeroU32::new(1000).unwrap();

    fn make_header_bytes(
        offset_x: u32,
        offset_y: u32,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Vec<u8> {
        Header::new(
            DataType::U64,
            DataType::U64,
            offset_x,
            offset_y,
            width,
            height,
        )
        .to_bytes()
        .to_vec()
    }

    fn make_range_bytes(gap: u64, len: u64) -> [u8; 16] {
        let mut buf = [0u8; 16];
        write_u64(&mut buf[..8], gap);
        write_u64(&mut buf[8..], len);
        buf
    }

    fn expect_unexpected_eof<T>(result: Result<T, ReadProtocolError>) {
        let Err(ReadProtocolError::Io(e)) = result else {
            panic!("Expected io Error")
        };
        assert_eq!(e.kind(), ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn roundtrip() {
        let ranges = (0..100).map(|i| {
            let s = (i as u64) * 20;
            s..=s + 5 + (i as u64 % 10)
        });
        let expected: Vec<_> = ranges
            .clone()
            .map(|r| NonZeroRange::new(*r.start()..*r.end() + 1))
            .collect();
        let mut buf = Vec::new();
        let stream = futures_util::stream::iter(ranges);
        let writer =
            AsyncRangeWriter::new(&mut buf, stream, 0, 0, NonZeroU32::MIN, NonZeroU32::MIN);
        writer.await.unwrap();
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert_eq!(expected, result);
    }

    #[tokio::test]
    async fn read_empty_error() {
        let reader = AsyncRangeStream::new(&[][..]);
        let result = reader.try_collect::<Vec<_>>().await;
        expect_unexpected_eof(result);
    }

    #[tokio::test]
    async fn write_empty_error() {
        let input: [RangeInclusive<u64>; 0] = [];
        let writer = Vec::new();
        let _err = AsyncRangeWriter::new(writer, futures_util::stream::iter(input))
            .await
            .unwrap_err();
    }

    #[tokio::test]
    async fn header_only_is_empty() {
        let buf = make_header_bytes(0, 0, NonZeroU32::MIN, NonZeroU32::MIN);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn overlapping_ranges_error() {
        let mut buf = Vec::new();
        let ranges = vec![10..=20u64, 15..=25];
        let writer = AsyncRangeWriter::new(
            &mut buf,
            futures_util::stream::iter(ranges),
            0,
            0,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        );
        let result = writer.await;
        assert!(matches!(result, Err(WriteProtocolError::RangeOrder { .. })));
    }

    #[tokio::test]
    async fn out_of_order_ranges_error() {
        let mut buf = Vec::new();
        let ranges = vec![50..=60u64, 10..=20];
        let writer = AsyncRangeWriter::new(
            &mut buf,
            futures_util::stream::iter(ranges),
            0,
            0,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        );
        let result = writer.await;
        assert!(matches!(result, Err(WriteProtocolError::RangeOrder { .. })));
    }

    #[tokio::test]
    async fn invalid_protocol_version() {
        let mut buf = vec![0x99, 0x00, 0x00];
        buf.resize(HEADER_SIZE, 0);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(
            result,
            Err(ReadProtocolError::UnsupportedVersion(0x99))
        ));
    }

    #[tokio::test]
    async fn invalid_included_data_type() {
        let mut buf = vec![PROTOCOL_VERSION, 0x05, 0x00];
        buf.resize(HEADER_SIZE, 0);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(
            result,
            Err(ReadProtocolError::UnsupportedDataType(0x05))
        ));
    }

    #[tokio::test]
    async fn invalid_excluded_data_type() {
        let mut buf = vec![PROTOCOL_VERSION, 0x00, 0xFF];
        buf.resize(HEADER_SIZE, 0);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        assert!(matches!(
            result,
            Err(ReadProtocolError::UnsupportedDataType(0xFF))
        ));
    }

    #[tokio::test]
    async fn truncated_header_one_byte() {
        let buf = vec![PROTOCOL_VERSION];
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        expect_unexpected_eof(result);
    }

    #[tokio::test]
    async fn truncated_header_two_bytes() {
        let buf = vec![PROTOCOL_VERSION, 0x00];
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        expect_unexpected_eof(result);
    }

    #[tokio::test]
    async fn truncated_range_partial_gap() {
        let mut buf = make_header_bytes(0, 0, NonZeroU32::MIN, NonZeroU32::MIN);
        buf.extend_from_slice(&[0x01, 0x02, 0x03, 0x04]);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        expect_unexpected_eof(result);
    }

    #[tokio::test]
    async fn truncated_range_gap_complete_len_partial() {
        let mut buf = make_header_bytes(0, 0, NonZeroU32::MIN, NonZeroU32::MIN);
        buf.extend_from_slice(&make_range_bytes(10, 100)[..12]);
        let reader = AsyncRangeStream::new(&buf[..]);
        let result = reader.try_collect::<Vec<_>>().await;
        expect_unexpected_eof(result);
    }

    #[tokio::test]
    async fn single_range_roundtrip() {
        let ranges = vec![100..=200u64];
        let mut buf = Vec::new();
        let writer = AsyncRangeWriter::new(
            &mut buf,
            futures_util::stream::iter(ranges),
            0,
            0,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        );
        writer.await.unwrap();
        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();
        assert_eq!(result, vec![NonZeroRange::new(100u64..201)]);
    }

    #[tokio::test]
    async fn adjacent_ranges_roundtrip() {
        let ranges = vec![10..=20u64, 21..=30];
        let mut buf = Vec::new();
        let writer = AsyncRangeWriter::new(
            &mut buf,
            futures_util::stream::iter(ranges),
            0,
            0,
            NONZERO_1000,
            NONZERO_1000,
        );
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
        let mut buf = make_header_bytes(0, 0, NONZERO_1000, NONZERO_1000);
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
        assert!(matches!(result, Err(ReadProtocolError::Io(_))));
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
        let async_writer = AsyncRangeWriter::new(
            writer,
            futures_util::stream::iter(ranges),
            0,
            0,
            NonZeroU32::MIN,
            NonZeroU32::MIN,
        );
        let result = async_writer.await;
        let Err(WriteProtocolError::Io(e)) = result else {
            panic!("Expected Custom io error");
        };
        assert_eq!(e.kind(), ErrorKind::Other);
    }

    #[tokio::test]
    async fn offset_zero_produces_same_ranges() {
        let global_ranges: Vec<RangeInclusive<u64>> = vec![5..=10, 100..=200];
        let mut buf = Vec::new();
        let writer = AsyncRangeWriter::new(
            &mut buf,
            futures_util::stream::iter(global_ranges.clone()),
            0,
            0,
            NONZERO_1000,
            NONZERO_1000,
        );
        writer.await.unwrap();

        let reader = AsyncRangeStream::new(&buf[..]);
        let result: Vec<_> = reader.try_collect().await.unwrap();

        let expected: Vec<_> = global_ranges
            .iter()
            .map(|r| NonZeroRange::new(*r.start()..*r.end() + 1))
            .collect();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn rectangle_offset_local_single_element() {
        use crate::Rect;

        let offset_x = 3;
        let offset_y = 5;
        let width = NonZeroU32::new(20).unwrap();
        let height = NonZeroU32::new(3).unwrap();

        let rect = Rect::new(
            offset_x,
            offset_y,
            NonZeroU32::new(width.get() - offset_x).unwrap(),
            height,
        );
        let global_ranges: Vec<RangeInclusive<u64>> = rect
            .into_rect_iter::<RangeInclusive<u32>>(width)
            .map(|r| *r.start() as u64..=*r.end() as u64)
            .collect();

        assert_eq!(global_ranges, vec![103..=119, 123..=139, 143..=159]);

        let mut buf = Vec::new();
        let writer = AsyncRangeWriter::new(
            &mut buf,
            futures_util::stream::iter(global_ranges.clone()),
            offset_x,
            offset_y,
            width,
            height,
        );
        writer.await.unwrap();

        let local_reader = AsyncRangeStream::new(&buf[..]).into_iter_local();
        let local_result: Vec<_> = local_reader.try_collect().await.unwrap();
        assert_eq!(local_result, vec![NonZeroRange::new(0u64..(3 * 17))]);

        let global_reader = AsyncRangeStream::new(&buf[..]);
        let global_result: Vec<_> = global_reader
            .map_ok(RangeInclusive::<u64>::from)
            .try_collect()
            .await
            .unwrap();
        assert_eq!(global_result, global_ranges);
    }
}
