//! Zero-Copy Snapshot Implementation
//!
//! This module implements the zero-copy snapshot design that leverages immutable storage files
//! for ultra-fast snapshot creation and restoration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use tokio_stream::Stream;

use crate::{CompressionType, StorageError};

/// Storage with zero-copy snapshot capability
#[async_trait]
pub trait SnapshotCapableStorage: crate::StorageBackend {
    type SnapshotData: Clone + Send + Sync;
    type SequenceNumber: Copy + Send + Sync + Ord;

    // ===== ZERO-COPY OPERATIONS (Local) =====
    /// Create zero-copy snapshot of current data
    async fn create_zero_copy_snapshot(
        &self,
    ) -> Result<ZeroCopySnapshot<Self::SnapshotData>, StorageError>;

    /// Restore from zero-copy snapshot
    async fn restore_from_snapshot(
        &self,
        snapshot: &ZeroCopySnapshot<Self::SnapshotData>,
    ) -> Result<(), StorageError>;

    async fn latest_snapshot(
        &self,
    ) -> Result<Option<ZeroCopySnapshot<Self::SnapshotData>>, StorageError>;

    /// Get snapshot size estimation
    async fn estimate_snapshot_size(
        &self,
        snapshot_data: &Self::SnapshotData,
    ) -> Result<u64, StorageError>;

    // ===== RAFT INTEGRATION METHODS (KV STREAMING) =====
    /// Create a streaming iterator of key-value pairs for Raft transmission
    async fn create_kv_snapshot_stream(
        &self,
        snapshot: &ZeroCopySnapshot<Self::SnapshotData>,
    ) -> Result<
        Box<
            dyn Stream<Item = Result<(Vec<u8>, Vec<u8>), StorageError>>
                + Send
                + Unpin,
        >,
        StorageError,
    >;

    /// Install snapshot from streaming key-value pairs
    async fn install_kv_snapshot_from_stream<S>(
        &self,
        stream: S,
    ) -> Result<(), StorageError>
    where
        S: Stream<Item = Result<(Vec<u8>, Vec<u8>), StorageError>>
            + Send
            + Unpin;
}

/// Zero-copy snapshot containing references to immutable files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroCopySnapshot<F> {
    /// Unique snapshot identifier
    pub snapshot_id: String,
    /// Timestamp when snapshot was created
    pub created_at: SystemTime,
    /// Sequence number at snapshot time
    pub sequence_number: u64,
    /// Snapshot data (could be file references or in-memory data)
    pub snapshot_data: F,
    /// Total number of entries in the snapshot
    pub entry_count: u64,
    /// Total size in bytes
    pub total_size_bytes: u64,
    /// Compression type used for snapshot files
    pub compression: CompressionType,
}

/// Metadata for a storage file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    /// File size in bytes
    pub size_bytes: u64,
    /// Number of entries in the file
    pub entry_count: u64,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Checksum for integrity verification
    pub checksum: String,
}
