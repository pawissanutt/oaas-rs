use std::io::{Result as IoResult, SeekFrom};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
use tokio::sync::Mutex;

/// A streaming snapshot buffer that satisfies OpenRaft's requirements
/// while supporting efficient streaming operations
#[derive(Debug, Clone)]
pub struct StreamingSnapshotBuffer {
    inner: Arc<Mutex<StreamingSnapshotInner>>,
}

impl Default for StreamingSnapshotBuffer {
    fn default() -> Self {
        Self::new()
    }
}

struct StreamingSnapshotInner {
    /// Current data buffer (grows as data is written)
    buffer: Vec<u8>,
    /// Current read position
    read_pos: u64,
    /// Current write position
    write_pos: u64,
    /// Whether the snapshot is complete (no more writes expected)
    is_complete: bool,
    /// Optional streaming source for lazy loading
    stream_source: Option<Box<dyn AsyncRead + Send + Unpin>>,
}

impl std::fmt::Debug for StreamingSnapshotInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingSnapshotInner")
            .field("buffer_len", &self.buffer.len())
            .field("read_pos", &self.read_pos)
            .field("write_pos", &self.write_pos)
            .field("is_complete", &self.is_complete)
            .field("has_stream_source", &self.stream_source.is_some())
            .finish()
    }
}

impl StreamingSnapshotBuffer {
    /// Create a new empty streaming snapshot buffer
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(StreamingSnapshotInner {
                buffer: Vec::new(),
                read_pos: 0,
                write_pos: 0,
                is_complete: false,
                stream_source: None,
            })),
        }
    }

    /// Create a streaming snapshot buffer with a streaming source
    pub fn with_stream(stream: Box<dyn AsyncRead + Send + Unpin>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(StreamingSnapshotInner {
                buffer: Vec::new(),
                read_pos: 0,
                write_pos: 0,
                is_complete: false,
                stream_source: Some(stream),
            })),
        }
    }

    /// Create a streaming snapshot buffer from existing data
    pub fn from_bytes(data: Vec<u8>) -> Self {
        let write_pos = data.len() as u64;

        Self {
            inner: Arc::new(Mutex::new(StreamingSnapshotInner {
                buffer: data,
                read_pos: 0,
                write_pos,
                is_complete: true,
                stream_source: None,
            })),
        }
    }

    /// Mark the snapshot as complete (no more writes)
    pub async fn mark_complete(&self) {
        let mut inner = self.inner.lock().await;
        inner.is_complete = true;
    }

    /// Get the current size of the snapshot
    pub async fn len(&self) -> u64 {
        let inner = self.inner.lock().await;
        inner.write_pos
    }

    /// Check whether the snapshot currently holds any bytes
    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.lock().await;
        inner.write_pos == 0
    }
}

impl AsyncRead for StreamingSnapshotBuffer {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<IoResult<()>> {
        let mut inner = match self.inner.try_lock() {
            Ok(inner) => inner,
            Err(_) => {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
        };

        // If we have data in buffer, read from it
        let available_data =
            (inner.write_pos as usize).saturating_sub(inner.read_pos as usize);
        if available_data > 0 {
            let start = inner.read_pos as usize;
            let end =
                std::cmp::min(start + buf.remaining(), inner.buffer.len());
            let to_read = &inner.buffer[start..end];

            buf.put_slice(to_read);
            inner.read_pos += to_read.len() as u64;

            return Poll::Ready(Ok(()));
        }

        // If no data available but stream is complete, return EOF
        if inner.is_complete {
            return Poll::Ready(Ok(()));
        }

        // If we have a stream source, try to read from it
        if let Some(ref mut stream) = inner.stream_source {
            // Try to read more data from the stream
            let mut temp_buf = vec![0u8; 8192]; // 8KB chunks
            let mut temp_read_buf = ReadBuf::new(&mut temp_buf);

            match Pin::new(stream).poll_read(cx, &mut temp_read_buf) {
                Poll::Ready(Ok(())) => {
                    let bytes_read = temp_read_buf.filled().len();
                    if bytes_read == 0 {
                        // Stream is exhausted
                        inner.is_complete = true;
                        inner.stream_source = None;
                        return Poll::Ready(Ok(()));
                    }

                    // Add data to buffer
                    inner.buffer.extend_from_slice(
                        &temp_read_buf.filled()[..bytes_read],
                    );
                    inner.write_pos += bytes_read as u64;

                    // Now try reading again
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        } else {
            // No stream source and no data - wait for writes
            Poll::Pending
        }
    }
}

impl AsyncWrite for StreamingSnapshotBuffer {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<IoResult<usize>> {
        let mut inner = match self.inner.try_lock() {
            Ok(inner) => inner,
            Err(_) => return Poll::Pending,
        };

        // Extend buffer at write position
        if inner.write_pos as usize == inner.buffer.len() {
            // Writing at the end - simple append
            inner.buffer.extend_from_slice(buf);
        } else {
            // Writing in the middle - need to insert/overwrite
            let write_pos = inner.write_pos as usize;
            if write_pos + buf.len() <= inner.buffer.len() {
                // Overwrite existing data
                inner.buffer[write_pos..write_pos + buf.len()]
                    .copy_from_slice(buf);
            } else {
                // Extend buffer and write
                inner.buffer.resize(write_pos + buf.len(), 0);
                inner.buffer[write_pos..write_pos + buf.len()]
                    .copy_from_slice(buf);
            }
        }

        inner.write_pos += buf.len() as u64;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<IoResult<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<IoResult<()>> {
        let mut inner = match self.inner.try_lock() {
            Ok(inner) => inner,
            Err(_) => return Poll::Pending,
        };
        inner.is_complete = true;
        Poll::Ready(Ok(()))
    }
}

impl AsyncSeek for StreamingSnapshotBuffer {
    fn start_seek(self: Pin<&mut Self>, position: SeekFrom) -> IoResult<()> {
        let mut inner = self.inner.try_lock().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::WouldBlock,
                "Lock contention",
            )
        })?;

        let new_pos = match position {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(offset) => {
                if offset >= 0 {
                    inner.write_pos + offset as u64
                } else {
                    inner.write_pos.saturating_sub((-offset) as u64)
                }
            }
            SeekFrom::Current(offset) => {
                if offset >= 0 {
                    inner.read_pos + offset as u64
                } else {
                    inner.read_pos.saturating_sub((-offset) as u64)
                }
            }
        };

        inner.read_pos = new_pos;
        Ok(())
    }

    fn poll_complete(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<IoResult<u64>> {
        let inner = match self.inner.try_lock() {
            Ok(inner) => inner,
            Err(_) => return Poll::Pending,
        };
        Poll::Ready(Ok(inner.read_pos))
    }
}
