//! Raft Snapshot Integration
//!
//! Direct integration between storage snapshots and OpenRaft without intermediate layers.

use openraft::{Snapshot, SnapshotMeta};
use oprc_dp_storage::{SnapshotCapableStorage, StorageError};
use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, ReadBuf};
use tokio_stream::Stream;

use crate::replication::raft::{
    ReplicationTypeConfig, StreamingSnapshotBuffer,
};

/// Direct Raft snapshot creation from storage
pub async fn create_raft_snapshot<S>(
    storage: &S,
    last_applied_log_id: Option<openraft::LogId<u64>>,
    last_membership: openraft::StoredMembership<u64, openraft::BasicNode>,
    snapshot_idx: u64,
) -> Result<Snapshot<ReplicationTypeConfig>, StorageError>
where
    S: SnapshotCapableStorage,
{
    // Create storage snapshot
    let storage_snapshot = storage.create_snapshot().await?;

    create_raft_snapshot_from_existing(
        storage,
        &storage_snapshot,
        last_applied_log_id,
        last_membership,
        snapshot_idx,
    )
    .await
}

/// Create Raft snapshot from existing storage snapshot
pub async fn create_raft_snapshot_from_existing<S>(
    storage: &S,
    storage_snapshot: &oprc_dp_storage::Snapshot<S::SnapshotData>,
    last_applied_log_id: Option<openraft::LogId<u64>>,
    last_membership: openraft::StoredMembership<u64, openraft::BasicNode>,
    snapshot_idx: u64,
) -> Result<Snapshot<ReplicationTypeConfig>, StorageError>
where
    S: SnapshotCapableStorage,
{
    // Create unique snapshot ID
    let snapshot_id = if let Some(last) = last_applied_log_id {
        format!("{}-{}-{}", last.leader_id, last.index, snapshot_idx)
    } else {
        format!("--{}", snapshot_idx)
    };

    // Create streaming reader directly from storage
    let kv_stream = storage.create_kv_snapshot_stream(storage_snapshot).await?;
    let streaming_reader = create_streaming_reader(kv_stream).await?;

    // Create streaming snapshot buffer
    let streaming_snapshot =
        StreamingSnapshotBuffer::with_stream(streaming_reader);

    let snapshot_meta = SnapshotMeta {
        last_log_id: last_applied_log_id,
        last_membership,
        snapshot_id,
    };

    Ok(Snapshot {
        meta: snapshot_meta,
        snapshot: Box::new(streaming_snapshot),
    })
}

/// Install Raft snapshot directly to storage
pub async fn install_raft_snapshot<S>(
    storage: &S,
    snapshot: StreamingSnapshotBuffer,
) -> Result<(), StorageError>
where
    S: SnapshotCapableStorage,
{
    // Convert streaming snapshot to key-value stream
    let kv_stream = create_kv_stream_from_reader(snapshot).await?;

    // Install directly to storage
    storage.install_kv_snapshot_from_stream(kv_stream).await
}

/// Create a streaming reader from key-value stream
async fn create_streaming_reader(
    kv_stream: Box<
        dyn Stream<
                Item = Result<
                    (
                        oprc_dp_storage::StorageValue,
                        oprc_dp_storage::StorageValue,
                    ),
                    oprc_dp_storage::StorageError,
                >,
            > + Send
            + Unpin,
    >,
) -> Result<Box<dyn AsyncRead + Send + Unpin>, oprc_dp_storage::StorageError> {
    // Return a true streaming reader that processes data on-demand
    Ok(Box::new(KvStreamingReader::new(kv_stream)))
}

/// Create key-value stream from streaming reader
async fn create_kv_stream_from_reader(
    reader: StreamingSnapshotBuffer,
) -> Result<
    impl Stream<
        Item = Result<
            (oprc_dp_storage::StorageValue, oprc_dp_storage::StorageValue),
            oprc_dp_storage::StorageError,
        >,
    >,
    oprc_dp_storage::StorageError,
> {
    // Return a true streaming parser that processes data on-demand
    Ok(KvStreamingParser::new(reader))
}

/// True streaming reader that converts key-value stream to AsyncRead on-demand
struct KvStreamingReader {
    kv_stream: Box<
        dyn Stream<
                Item = Result<
                    (
                        oprc_dp_storage::StorageValue,
                        oprc_dp_storage::StorageValue,
                    ),
                    oprc_dp_storage::StorageError,
                >,
            > + Send
            + Unpin,
    >,
    current_item: Option<Cursor<Vec<u8>>>,
    stream_ended: bool,
}

impl KvStreamingReader {
    fn new(
        kv_stream: Box<
            dyn Stream<
                    Item = Result<
                        (
                            oprc_dp_storage::StorageValue,
                            oprc_dp_storage::StorageValue,
                        ),
                        oprc_dp_storage::StorageError,
                    >,
                > + Send
                + Unpin,
        >,
    ) -> Self {
        Self {
            kv_stream,
            current_item: None,
            stream_ended: false,
        }
    }

    fn serialize_kv_pair(
        key: &oprc_dp_storage::StorageValue,
        value: &oprc_dp_storage::StorageValue,
    ) -> Vec<u8> {
        let mut serialized = Vec::new();
        let key_slice = key.as_slice();
        let value_slice = value.as_slice();
        serialized.extend_from_slice(&(key_slice.len() as u32).to_le_bytes());
        serialized.extend_from_slice(key_slice);
        serialized.extend_from_slice(&(value_slice.len() as u32).to_le_bytes());
        serialized.extend_from_slice(value_slice);
        serialized
    }
}

impl AsyncRead for KvStreamingReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            // Try to read from current item if available
            if let Some(ref mut cursor) = self.current_item {
                match Pin::new(cursor).poll_read(cx, buf) {
                    Poll::Ready(Ok(())) if buf.filled().is_empty() => {
                        // Current item exhausted, remove it and continue to next
                        self.current_item = None;
                        continue;
                    }
                    other => return other,
                }
            }

            // If stream has ended, return EOF
            if self.stream_ended {
                return Poll::Ready(Ok(()));
            }

            // Get next item from stream
            match Pin::new(&mut self.kv_stream).poll_next(cx) {
                Poll::Ready(Some(Ok((key, value)))) => {
                    // Serialize the key-value pair and create cursor
                    let serialized = Self::serialize_kv_pair(&key, &value);
                    self.current_item = Some(Cursor::new(serialized));
                    continue; // Now read from this cursor
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::other(
                        e.to_string(),
                    )));
                }
                Poll::Ready(None) => {
                    self.stream_ended = true;
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// True streaming parser that converts AsyncRead to key-value stream on-demand
struct KvStreamingParser {
    reader: StreamingSnapshotBuffer,
    buffer: Vec<u8>,
    state: ParseState,
}

#[derive(Debug)]
enum ParseState {
    KeyLen,
    Key(usize),
    ValueLen,
    Value(oprc_dp_storage::StorageValue, usize),
}

impl KvStreamingParser {
    fn new(reader: StreamingSnapshotBuffer) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
            state: ParseState::KeyLen,
        }
    }
}

impl Stream for KvStreamingParser {
    type Item = Result<
        (oprc_dp_storage::StorageValue, oprc_dp_storage::StorageValue),
        oprc_dp_storage::StorageError,
    >;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            match std::mem::replace(&mut self.state, ParseState::KeyLen) {
                ParseState::KeyLen => {
                    let mut key_len_bytes = [0u8; 4];
                    let mut read_buf = ReadBuf::new(&mut key_len_bytes);

                    match Pin::new(&mut self.reader)
                        .poll_read(cx, &mut read_buf)
                    {
                        Poll::Ready(Ok(())) if read_buf.filled().len() == 4 => {
                            let key_len =
                                u32::from_le_bytes(key_len_bytes) as usize;
                            if key_len == 0 {
                                return Poll::Ready(None); // End of stream
                            }
                            self.state = ParseState::Key(key_len);
                        }
                        Poll::Ready(Ok(())) if read_buf.filled().is_empty() => {
                            return Poll::Ready(None); // EOF
                        }
                        Poll::Ready(Ok(())) => {
                            // Partial read, need to handle this properly
                            self.state = ParseState::KeyLen;
                            return Poll::Pending;
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                oprc_dp_storage::StorageError::backend(
                                    e.to_string(),
                                ),
                            )));
                        }
                        Poll::Pending => {
                            self.state = ParseState::KeyLen;
                            return Poll::Pending;
                        }
                    }
                }
                ParseState::Key(key_len) => {
                    let mut key = vec![0u8; key_len];
                    let mut read_buf = ReadBuf::new(&mut key);

                    match Pin::new(&mut self.reader)
                        .poll_read(cx, &mut read_buf)
                    {
                        Poll::Ready(Ok(()))
                            if read_buf.filled().len() == key_len =>
                        {
                            self.buffer = key;
                            self.state = ParseState::ValueLen;
                        }
                        Poll::Ready(Ok(())) => {
                            // Partial read
                            self.state = ParseState::Key(key_len);
                            return Poll::Pending;
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                oprc_dp_storage::StorageError::backend(
                                    e.to_string(),
                                ),
                            )));
                        }
                        Poll::Pending => {
                            self.state = ParseState::Key(key_len);
                            return Poll::Pending;
                        }
                    }
                }
                ParseState::ValueLen => {
                    let mut value_len_bytes = [0u8; 4];
                    let mut read_buf = ReadBuf::new(&mut value_len_bytes);

                    match Pin::new(&mut self.reader)
                        .poll_read(cx, &mut read_buf)
                    {
                        Poll::Ready(Ok(())) if read_buf.filled().len() == 4 => {
                            let value_len =
                                u32::from_le_bytes(value_len_bytes) as usize;
                            let key = std::mem::take(&mut self.buffer);
                            self.state = ParseState::Value(
                                oprc_dp_storage::StorageValue::from(key),
                                value_len,
                            );
                        }
                        Poll::Ready(Ok(())) => {
                            // Partial read
                            self.state = ParseState::ValueLen;
                            return Poll::Pending;
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                oprc_dp_storage::StorageError::backend(
                                    e.to_string(),
                                ),
                            )));
                        }
                        Poll::Pending => {
                            self.state = ParseState::ValueLen;
                            return Poll::Pending;
                        }
                    }
                }
                ParseState::Value(key, value_len) => {
                    let mut value = vec![0u8; value_len];
                    let mut read_buf = ReadBuf::new(&mut value);

                    match Pin::new(&mut self.reader)
                        .poll_read(cx, &mut read_buf)
                    {
                        Poll::Ready(Ok(()))
                            if read_buf.filled().len() == value_len =>
                        {
                            self.state = ParseState::KeyLen;
                            return Poll::Ready(Some(Ok((
                                key,
                                oprc_dp_storage::StorageValue::from(value),
                            ))));
                        }
                        Poll::Ready(Ok(())) => {
                            // Partial read
                            self.state = ParseState::Value(key, value_len);
                            return Poll::Pending;
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                oprc_dp_storage::StorageError::backend(
                                    e.to_string(),
                                ),
                            )));
                        }
                        Poll::Pending => {
                            self.state = ParseState::Value(key, value_len);
                            return Poll::Pending;
                        }
                    }
                }
            }
        }
    }
}
