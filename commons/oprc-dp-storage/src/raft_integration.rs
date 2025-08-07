//! Raft Integration for Zero-Copy Snapshots
//!
//! This module provides integration between zero-copy snapshots and OpenRaft,
//! enabling streaming key-value pairs for Raft network transmission.

use async_trait::async_trait;
use bincode::{config, decode_from_slice, encode_to_vec, Decode, Encode};
use futures::stream::BoxStream;
use futures::StreamExt as FuturesStreamExt;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_stream::Stream;

use crate::{
    snapshot::{SnapshotCapableStorage, ZeroCopySnapshot},
    StorageError,
};

/// Key-Value streaming snapshot that bridges zero-copy snapshots with OpenRaft
#[derive(Debug, Clone)]
pub struct KvStreamingRaftSnapshot<F> {
    /// Zero-copy snapshot data
    pub zero_copy_snapshot: ZeroCopySnapshot<F>,
    /// Raft log metadata (using generic types for flexibility)
    pub log_id: Option<String>, // Simplified for now
    /// Cluster membership at snapshot time
    pub membership: String, // Simplified for now
    /// Snapshot creation metadata
    pub snapshot_id: String,
    /// Serialization format version for compatibility
    pub format_version: u32,
}

impl<F> KvStreamingRaftSnapshot<F>
where
    F: Clone + Send + Sync,
{
    /// Create streaming reader that serializes key-value pairs for network transmission
    pub async fn create_kv_stream_reader<S>(
        &self,
        storage: &S,
    ) -> Result<Box<dyn AsyncRead + Send + Unpin>, StorageError>
    where
        S: SnapshotCapableStorage<SnapshotData = F>,
    {
        // Get key-value stream from storage
        let kv_stream = storage
            .create_kv_snapshot_stream(&self.zero_copy_snapshot)
            .await?;

        // Wrap with metadata header
        let metadata = SnapshotMetadata {
            log_id: self.log_id.clone(),
            membership: self.membership.clone(),
            snapshot_id: self.snapshot_id.clone(),
            format_version: self.format_version,
        };

        // Create composite stream: [metadata][serialized kv pairs]
        Ok(Box::new(KvSerializingStream::new(metadata, kv_stream)))
    }

    /// Extract zero-copy snapshot for local operations
    pub fn zero_copy_snapshot(&self) -> &ZeroCopySnapshot<F> {
        &self.zero_copy_snapshot
    }
}

/// Metadata portion of streaming snapshot
#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct SnapshotMetadata {
    pub log_id: Option<String>,
    pub membership: String,
    pub snapshot_id: String,
    pub format_version: u32,
}

/// Stream that serializes key-value pairs with metadata header
pub struct KvSerializingStream {
    header_cursor: Option<Cursor<Vec<u8>>>,
    kv_stream: BoxStream<'static, Result<(Vec<u8>, Vec<u8>), StorageError>>,
    current_item_cursor: Option<Cursor<Vec<u8>>>,
}

impl KvSerializingStream {
    pub fn new(
        metadata: SnapshotMetadata,
        kv_stream: Box<
            dyn Stream<Item = Result<(Vec<u8>, Vec<u8>), StorageError>>
                + Send
                + Unpin,
        >,
    ) -> Self {
        // Serialize metadata header
        let metadata_bytes = encode_to_vec(&metadata, config::standard())
            .expect("Failed to serialize metadata");

        let mut header_with_len = Vec::new();
        // Write header length as u32
        header_with_len
            .extend_from_slice(&(metadata_bytes.len() as u32).to_le_bytes());
        header_with_len.extend_from_slice(&metadata_bytes);

        Self {
            header_cursor: Some(Cursor::new(header_with_len)),
            kv_stream: FuturesStreamExt::boxed(kv_stream),
            current_item_cursor: None,
        }
    }

    fn serialize_kv_pair(key: &[u8], value: &[u8]) -> Vec<u8> {
        let mut serialized = Vec::new();
        // Format: [key_len: u32][key][value_len: u32][value]
        serialized.extend_from_slice(&(key.len() as u32).to_le_bytes());
        serialized.extend_from_slice(key);
        serialized.extend_from_slice(&(value.len() as u32).to_le_bytes());
        serialized.extend_from_slice(value);
        serialized
    }
}

impl AsyncRead for KvSerializingStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        // First read from header if available
        if let Some(ref mut cursor) = self.header_cursor {
            let pin_cursor = Pin::new(cursor);
            match pin_cursor.poll_read(cx, buf) {
                Poll::Ready(Ok(())) if !buf.filled().is_empty() => {
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Ok(())) => {
                    // Header fully consumed, remove it
                    self.header_cursor = None;
                }
                other => return other,
            }
        }

        loop {
            // Read from current item cursor if available
            if let Some(ref mut cursor) = self.current_item_cursor {
                let pin_cursor = Pin::new(cursor);
                match pin_cursor.poll_read(cx, buf) {
                    Poll::Ready(Ok(())) if !buf.filled().is_empty() => {
                        return Poll::Ready(Ok(()));
                    }
                    Poll::Ready(Ok(())) => {
                        // Current item fully consumed, remove it
                        self.current_item_cursor = None;
                        continue; // Try to get next item
                    }
                    other => return other,
                }
            }

            // Get next key-value pair from stream
            let pin_stream = Pin::new(&mut self.kv_stream);
            match pin_stream.poll_next(cx) {
                Poll::Ready(Some(Ok((key, value)))) => {
                    // Serialize the key-value pair
                    let serialized = Self::serialize_kv_pair(&key, &value);
                    self.current_item_cursor = Some(Cursor::new(serialized));
                    continue; // Now read from this cursor
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        e.to_string(),
                    )));
                }
                Poll::Ready(None) => {
                    // Stream ended
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

/// Stream that deserializes key-value pairs from binary data
pub struct KvDeserializingStream<R> {
    reader: R,
    buffer: Vec<u8>,
    state: DeserializingState,
}

#[derive(Debug)]
enum DeserializingState {
    ReadingKeyLen,
    ReadingKey(usize),
    ReadingValueLen,
    ReadingValue(Vec<u8>, usize),
}

impl<R: AsyncRead + Unpin> KvDeserializingStream<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
            state: DeserializingState::ReadingKeyLen,
        }
    }
}

impl<R: AsyncRead + Unpin> Stream for KvDeserializingStream<R> {
    type Item = Result<(Vec<u8>, Vec<u8>), StorageError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        loop {
            match std::mem::replace(
                &mut self.state,
                DeserializingState::ReadingKeyLen,
            ) {
                DeserializingState::ReadingKeyLen => {
                    let mut key_len_bytes = [0u8; 4];
                    let pin_reader = Pin::new(&mut self.reader);

                    match pin_reader
                        .poll_read(cx, &mut ReadBuf::new(&mut key_len_bytes))
                    {
                        Poll::Ready(Ok(())) => {
                            let key_len =
                                u32::from_le_bytes(key_len_bytes) as usize;
                            if key_len == 0 {
                                // End of stream marker
                                return Poll::Ready(None);
                            }
                            self.state =
                                DeserializingState::ReadingKey(key_len);
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                StorageError::backend(&e.to_string()),
                            )));
                        }
                        Poll::Pending => {
                            self.state = DeserializingState::ReadingKeyLen;
                            return Poll::Pending;
                        }
                    }
                }
                DeserializingState::ReadingKey(key_len) => {
                    let mut key = vec![0u8; key_len];
                    let pin_reader = Pin::new(&mut self.reader);

                    match pin_reader.poll_read(cx, &mut ReadBuf::new(&mut key))
                    {
                        Poll::Ready(Ok(())) => {
                            self.state = DeserializingState::ReadingValueLen;
                            // Store key for later use
                            self.buffer = key;
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                StorageError::backend(&e.to_string()),
                            )));
                        }
                        Poll::Pending => {
                            self.state =
                                DeserializingState::ReadingKey(key_len);
                            return Poll::Pending;
                        }
                    }
                }
                DeserializingState::ReadingValueLen => {
                    let mut value_len_bytes = [0u8; 4];
                    let pin_reader = Pin::new(&mut self.reader);

                    match pin_reader
                        .poll_read(cx, &mut ReadBuf::new(&mut value_len_bytes))
                    {
                        Poll::Ready(Ok(())) => {
                            let value_len =
                                u32::from_le_bytes(value_len_bytes) as usize;
                            let key = std::mem::take(&mut self.buffer);
                            self.state = DeserializingState::ReadingValue(
                                key, value_len,
                            );
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                StorageError::backend(&e.to_string()),
                            )));
                        }
                        Poll::Pending => {
                            self.state = DeserializingState::ReadingValueLen;
                            return Poll::Pending;
                        }
                    }
                }
                DeserializingState::ReadingValue(key, value_len) => {
                    let mut value = vec![0u8; value_len];
                    let pin_reader = Pin::new(&mut self.reader);

                    match pin_reader
                        .poll_read(cx, &mut ReadBuf::new(&mut value))
                    {
                        Poll::Ready(Ok(())) => {
                            self.state = DeserializingState::ReadingKeyLen;
                            return Poll::Ready(Some(Ok((key, value))));
                        }
                        Poll::Ready(Err(e)) => {
                            return Poll::Ready(Some(Err(
                                StorageError::backend(&e.to_string()),
                            )));
                        }
                        Poll::Pending => {
                            self.state = DeserializingState::ReadingValue(
                                key, value_len,
                            );
                            return Poll::Pending;
                        }
                    }
                }
            }
        }
    }
}

/// Extension trait for cursor-based snapshot installation
#[async_trait]
pub trait SnapshotCapableStorageExt: SnapshotCapableStorage {
    /// Install snapshot from cursor by deserializing key-value pairs
    async fn install_kv_snapshot_from_cursor<R>(
        &self,
        mut cursor: R,
    ) -> Result<(), StorageError>
    where
        R: AsyncRead + Send + Unpin,
    {
        // Read metadata header first
        let mut header_len_bytes = [0u8; 4];
        cursor
            .read_exact(&mut header_len_bytes)
            .await
            .map_err(|e| StorageError::backend(&e.to_string()))?;
        let header_len = u32::from_le_bytes(header_len_bytes) as usize;

        let mut metadata_bytes = vec![0u8; header_len];
        cursor
            .read_exact(&mut metadata_bytes)
            .await
            .map_err(|e| StorageError::backend(&e.to_string()))?;

        let _metadata: SnapshotMetadata =
            decode_from_slice(&metadata_bytes, config::standard())
                .map_err(|e| StorageError::backend(&e.to_string()))?
                .0;

        // Create key-value stream from remaining cursor data
        let kv_stream = KvDeserializingStream::new(cursor);

        // Install using the key-value stream
        self.install_kv_snapshot_from_stream(kv_stream).await
    }
}

impl<T: SnapshotCapableStorage> SnapshotCapableStorageExt for T {}
