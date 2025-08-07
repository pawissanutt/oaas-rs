use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use tokio_stream::Stream;

use crate::{CompressionType, StorageError, StorageValue};

/// Storage with zero-copy snapshot capability
#[async_trait]
pub trait SnapshotCapableStorage: crate::StorageBackend {
    type SnapshotData: Clone + Send + Sync;

    /// Create zero-copy snapshot of current data
    async fn create_snapshot(
        &self,
    ) -> Result<Snapshot<Self::SnapshotData>, StorageError>;

    /// Restore from zero-copy snapshot
    async fn restore_from_snapshot(
        &self,
        snapshot: &Snapshot<Self::SnapshotData>,
    ) -> Result<(), StorageError>;

    /// Get the latest snapshot if available
    async fn latest_snapshot(
        &self,
    ) -> Result<Option<Snapshot<Self::SnapshotData>>, StorageError>;

    /// Get snapshot size estimation
    async fn estimate_snapshot_size(
        &self,
        snapshot_data: &Self::SnapshotData,
    ) -> Result<u64, StorageError>;

    /// Create a streaming iterator of key-value pairs from a snapshot
    async fn create_kv_snapshot_stream(
        &self,
        snapshot: &Snapshot<Self::SnapshotData>,
    ) -> Result<
        Box<
            dyn Stream<
                    Item = Result<(StorageValue, StorageValue), StorageError>,
                > + Send
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
        S: Stream<Item = Result<(StorageValue, StorageValue), StorageError>>
            + Send
            + Unpin;
}

/// A zero-copy snapshot containing metadata and data reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot<F> {
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
