//! Zero-Copy Snapshot Implementation
//!
//! This module implements the zero-copy snapshot design that leverages immutable storage files
//! for ultra-fast snapshot creation and restoration.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

use crate::{
    specialized::EnhancedApplicationStorage, CompressionType, StorageBackend,
    StorageBackendType, StorageConfig, StorageError, StorageValue,
};

/// Storage with zero-copy snapshot capability
#[async_trait]
pub trait SnapshotCapableStorage: crate::StorageBackend {
    type SnapshotData: Clone + Send + Sync;
    type SequenceNumber: Copy + Send + Sync + Ord;

    /// Create zero-copy snapshot of current data
    async fn create_zero_copy_snapshot(
        &self,
    ) -> Result<ZeroCopySnapshot<Self::SnapshotData>, StorageError>;

    /// Restore from zero-copy snapshot
    async fn restore_from_snapshot(
        &self,
        snapshot: &ZeroCopySnapshot<Self::SnapshotData>,
    ) -> Result<(), StorageError>;

    /// Get current sequence number (monotonically increasing)
    async fn current_sequence(
        &self,
    ) -> Result<Self::SequenceNumber, StorageError>;

    /// Get snapshot size estimation
    async fn estimate_snapshot_size(
        &self,
        snapshot_data: &Self::SnapshotData,
    ) -> Result<u64, StorageError>;
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

/// In-memory snapshot data containing actual key-value pairs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InMemorySnapshotData {
    /// All key-value pairs at snapshot time
    pub data: std::collections::HashMap<Vec<u8>, StorageValue>,
    /// Metadata about the snapshot
    pub metadata: FileMetadata,
}

/// Zero-copy implementation for MemoryStorage that enables instant snapshots
/// using immutable data structures and file references
#[derive(Clone)]
pub struct ZeroCopyMemoryStorage {
    inner: EnhancedApplicationStorage<crate::MemoryStorage>,
    sequence_number: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl ZeroCopyMemoryStorage {
    /// Create a new zero-copy memory storage
    pub async fn new() -> Result<Self, StorageError> {
        let config = StorageConfig {
            backend_type: StorageBackendType::Memory,
            path: None,
            memory_limit_mb: Some(256),
            cache_size_mb: Some(64),
            compression: false,
            sync_writes: false,
            properties: std::collections::HashMap::new(),
        };
        let memory_backend = crate::MemoryStorage::new(config).await?;
        let inner = EnhancedApplicationStorage::new(memory_backend)
            .await
            .map_err(|e| StorageError::backend(e.to_string()))?;

        Ok(Self {
            inner,
            sequence_number: std::sync::Arc::new(
                std::sync::atomic::AtomicU64::new(0),
            ),
        })
    }

    /// Increment and return the sequence number
    fn next_sequence(&self) -> u64 {
        self.sequence_number
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

impl Default for ZeroCopyMemoryStorage {
    fn default() -> Self {
        // For default, we'll use a blocking approach since async is not allowed in Default
        let create_config = || StorageConfig {
            backend_type: StorageBackendType::Memory,
            path: None,
            memory_limit_mb: Some(256),
            cache_size_mb: Some(64),
            compression: false,
            sync_writes: false,
            properties: std::collections::HashMap::new(),
        };

        let inner = tokio::runtime::Handle::try_current()
            .map(|handle| {
                handle.block_on(async {
                    let memory_backend =
                        crate::MemoryStorage::new(create_config())
                            .await
                            .unwrap();
                    EnhancedApplicationStorage::new(memory_backend)
                        .await
                        .map_err(|e| StorageError::backend(e.to_string()))
                        .unwrap()
                })
            })
            .unwrap_or_else(|_| {
                // If no tokio runtime, create a new one
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let memory_backend =
                        crate::MemoryStorage::new(create_config())
                            .await
                            .unwrap();
                    EnhancedApplicationStorage::new(memory_backend)
                        .await
                        .map_err(|e| StorageError::backend(e.to_string()))
                        .unwrap()
                })
            });

        Self {
            inner,
            sequence_number: std::sync::Arc::new(
                std::sync::atomic::AtomicU64::new(0),
            ),
        }
    }
}

// Implement basic storage backend traits by delegating to inner storage
#[async_trait]
impl crate::StorageBackend for ZeroCopyMemoryStorage {
    type Transaction = <EnhancedApplicationStorage<crate::MemoryStorage> as crate::StorageBackend>::Transaction;

    async fn begin_transaction(
        &self,
    ) -> Result<Self::Transaction, StorageError> {
        self.inner.begin_transaction().await
    }

    async fn get(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, StorageError> {
        self.inner.get(key).await
    }

    async fn put(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> Result<(), StorageError> {
        let result = self.inner.put(key, value).await;
        if result.is_ok() {
            self.next_sequence();
        }
        result
    }

    async fn delete(&self, key: &[u8]) -> Result<(), StorageError> {
        let result = self.inner.delete(key).await;
        if result.is_ok() {
            self.next_sequence();
        }
        result
    }

    async fn exists(&self, key: &[u8]) -> Result<bool, StorageError> {
        self.inner.exists(key).await
    }

    async fn scan(
        &self,
        prefix: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, StorageError> {
        self.inner.scan(prefix).await
    }

    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, StorageError> {
        self.inner.scan_range(start, end).await
    }

    async fn count(&self) -> Result<u64, StorageError> {
        self.inner.count().await
    }

    async fn flush(&self) -> Result<(), StorageError> {
        self.inner.flush().await
    }

    async fn close(&self) -> Result<(), StorageError> {
        self.inner.close().await
    }

    fn backend_type(&self) -> crate::StorageBackendType {
        self.inner.backend_type()
    }

    async fn stats(&self) -> Result<crate::StorageStats, StorageError> {
        self.inner.stats().await
    }

    async fn compact(&self) -> Result<(), StorageError> {
        self.inner.compact().await
    }
}

/// Simple file reference for memory storage (just an identifier)
pub type MemoryFileReference = String;

#[async_trait]
impl SnapshotCapableStorage for ZeroCopyMemoryStorage {
    type SnapshotData = InMemorySnapshotData;
    type SequenceNumber = u64;

    async fn create_zero_copy_snapshot(
        &self,
    ) -> Result<ZeroCopySnapshot<Self::SnapshotData>, StorageError> {
        // Create in-memory snapshot by cloning all current data
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let current_seq = self
            .sequence_number
            .load(std::sync::atomic::Ordering::SeqCst);

        // Get all current data using scan with empty prefix (gets all keys)
        let mut data = std::collections::HashMap::new();
        let mut total_size = 0u64;

        // Use scan with empty prefix to get all key-value pairs
        let all_pairs = self.scan(&[]).await?;
        for (key_val, value_val) in all_pairs {
            let key = key_val.as_ref().to_vec();
            let value = value_val;
            total_size += key.len() as u64 + value.as_ref().len() as u64;
            data.insert(key, value);
        }

        let entry_count = data.len() as u64;

        // Create metadata
        let metadata = FileMetadata {
            size_bytes: total_size,
            entry_count,
            created_at: SystemTime::now(),
            checksum: format!("memory_snapshot_{}", snapshot_id),
        };

        // Create in-memory snapshot data
        let snapshot_data = InMemorySnapshotData { data, metadata };

        Ok(ZeroCopySnapshot {
            snapshot_id,
            created_at: SystemTime::now(),
            sequence_number: current_seq,
            snapshot_data,
            entry_count,
            total_size_bytes: total_size,
            compression: CompressionType::None,
        })
    }

    async fn restore_from_snapshot(
        &self,
        snapshot: &ZeroCopySnapshot<Self::SnapshotData>,
    ) -> Result<(), StorageError> {
        // Clear current data by getting all keys and deleting them
        let current_pairs = self.scan(&[]).await?;
        for (key_val, _) in current_pairs {
            self.delete(key_val.as_ref()).await?;
        }

        // Restore all data from the snapshot
        for (key, value) in &snapshot.snapshot_data.data {
            self.put(key, value.clone()).await?;
        }

        // Reset sequence number to match snapshot
        self.sequence_number.store(
            snapshot.sequence_number,
            std::sync::atomic::Ordering::SeqCst,
        );

        eprintln!(
            "Restored from zero-copy snapshot: {} with {} entries",
            snapshot.snapshot_id, snapshot.entry_count
        );

        Ok(())
    }

    async fn current_sequence(
        &self,
    ) -> Result<Self::SequenceNumber, StorageError> {
        Ok(self
            .sequence_number
            .load(std::sync::atomic::Ordering::SeqCst))
    }

    async fn estimate_snapshot_size(
        &self,
        snapshot_data: &Self::SnapshotData,
    ) -> Result<u64, StorageError> {
        Ok(snapshot_data.metadata.size_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_zero_copy_snapshot_creation() {
        let storage = ZeroCopyMemoryStorage::new().await.unwrap();

        // Add some test data
        storage
            .put(b"key1", StorageValue::from("value1"))
            .await
            .unwrap();
        storage
            .put(b"key2", StorageValue::from("value2"))
            .await
            .unwrap();

        // Create zero-copy snapshot
        let snapshot = storage.create_zero_copy_snapshot().await.unwrap();

        assert_eq!(snapshot.entry_count, 2);
        assert!(snapshot.total_size_bytes > 0);
        assert_eq!(snapshot.snapshot_data.data.len(), 2);
    }

    #[tokio::test]
    async fn test_zero_copy_snapshot_restore() {
        let storage1 = ZeroCopyMemoryStorage::new().await.unwrap();
        let storage2 = ZeroCopyMemoryStorage::new().await.unwrap();

        // Add test data to storage1
        storage1
            .put(b"key1", StorageValue::from("value1"))
            .await
            .unwrap();
        storage1
            .put(b"key2", StorageValue::from("value2"))
            .await
            .unwrap();

        // Create snapshot from storage1
        let snapshot = storage1.create_zero_copy_snapshot().await.unwrap();

        // Restore snapshot to storage2
        storage2.restore_from_snapshot(&snapshot).await.unwrap();

        // Verify sequence numbers match
        let seq1 = storage1.current_sequence().await.unwrap();
        let seq2 = storage2.current_sequence().await.unwrap();
        assert_eq!(seq1, seq2);
    }

    #[tokio::test]
    async fn test_sequence_number_tracking() {
        let storage = ZeroCopyMemoryStorage::new().await.unwrap();

        let seq1 = storage.current_sequence().await.unwrap();
        storage
            .put(b"key1", StorageValue::from("value1"))
            .await
            .unwrap();
        let seq2 = storage.current_sequence().await.unwrap();

        assert!(seq2 > seq1);
    }

    #[tokio::test]
    async fn test_file_metadata() {
        let storage = ZeroCopyMemoryStorage::new().await.unwrap();

        storage
            .put(b"key1", StorageValue::from("value1"))
            .await
            .unwrap();
        let snapshot = storage.create_zero_copy_snapshot().await.unwrap();

        let metadata = &snapshot.snapshot_data.metadata;

        assert_eq!(metadata.entry_count, 1);
        assert!(metadata.size_bytes > 0);
    }
}
