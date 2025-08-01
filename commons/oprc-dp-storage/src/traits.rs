use crate::{StorageError, StorageResult, StorageValue};
use async_trait::async_trait;

/// Core storage backend trait that abstracts different storage engines
#[async_trait]
pub trait StorageBackend: Send + Sync {
    type Transaction: StorageTransaction;

    /// Begin a new transaction
    async fn begin_transaction(&self) -> StorageResult<Self::Transaction>;

    /// Get a value by key
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>>;

    /// Put a key-value pair
    async fn put(&self, key: &[u8], value: StorageValue) -> StorageResult<()>;

    /// Delete a key
    async fn delete(&self, key: &[u8]) -> StorageResult<()>;

    /// Check if a key exists
    async fn exists(&self, key: &[u8]) -> StorageResult<bool>;

    /// Scan keys with a prefix
    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Scan keys in a range
    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Get the number of entries
    async fn count(&self) -> StorageResult<u64>;

    /// Flush any pending writes
    async fn flush(&self) -> StorageResult<()>;

    /// Close the storage backend
    async fn close(&self) -> StorageResult<()>;

    /// Get backend type information
    fn backend_type(&self) -> StorageBackendType;

    /// Get backend statistics
    async fn stats(&self) -> StorageResult<StorageStats>;

    /// Compact/optimize the storage (if supported)
    async fn compact(&self) -> StorageResult<()>;
}

/// Transaction trait for atomic operations
#[async_trait]
pub trait StorageTransaction: Send + Sync + Sized {
    /// Get a value by key within the transaction
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>>;

    /// Put a key-value pair within the transaction
    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()>;

    /// Delete a key within the transaction
    async fn delete(&mut self, key: &[u8]) -> StorageResult<()>;

    /// Check if a key exists within the transaction
    async fn exists(&self, key: &[u8]) -> StorageResult<bool>;

    /// Commit the transaction
    async fn commit(self) -> StorageResult<()>;

    /// Rollback the transaction
    async fn rollback(self) -> StorageResult<()>;

    /// Batch put operations
    async fn batch_put(
        &mut self,
        entries: Vec<(&[u8], StorageValue)>,
    ) -> StorageResult<()> {
        for (key, value) in entries {
            self.put(key, value).await?;
        }
        Ok(())
    }

    /// Batch delete operations
    async fn batch_delete(&mut self, keys: Vec<&[u8]>) -> StorageResult<()> {
        for key in keys {
            self.delete(key).await?;
        }
        Ok(())
    }
}

/// Storage backend types
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StorageBackendType {
    Memory,
    Redb,
    Fjall,
    RocksDb,
}

impl std::fmt::Display for StorageBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, "memory"),
            Self::Redb => write!(f, "redb"),
            Self::Fjall => write!(f, "fjall"),
            Self::RocksDb => write!(f, "rocksdb"),
        }
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub entries_count: u64,
    pub total_size_bytes: u64,
    pub memory_usage_bytes: Option<u64>,
    pub disk_usage_bytes: Option<u64>,
    pub cache_hit_rate: Option<f64>,
    pub backend_specific: std::collections::HashMap<String, String>,
}

impl Default for StorageStats {
    fn default() -> Self {
        Self {
            entries_count: 0,
            total_size_bytes: 0,
            memory_usage_bytes: None,
            disk_usage_bytes: None,
            cache_hit_rate: None,
            backend_specific: std::collections::HashMap::new(),
        }
    }
}

/// Batch operation for efficient bulk operations
#[derive(Debug, Clone)]
pub enum BatchOperation {
    Put {
        key: StorageValue,
        value: StorageValue,
    },
    Delete {
        key: StorageValue,
    },
}

/// Batch write trait for efficient bulk operations
#[async_trait]
pub trait BatchWrite {
    /// Execute a batch of operations atomically
    async fn batch_write(
        &self,
        operations: Vec<BatchOperation>,
    ) -> StorageResult<()>;

    /// Get optimal batch size for this backend
    fn optimal_batch_size(&self) -> usize {
        1000 // Default batch size
    }
}

/// Storage backend extension trait for advanced features
#[async_trait]
pub trait StorageBackendExt: StorageBackend {
    /// Create a snapshot of the current state
    async fn create_snapshot(&self) -> StorageResult<StorageSnapshot>;

    /// Restore from a snapshot
    async fn restore_snapshot(
        &self,
        snapshot: StorageSnapshot,
    ) -> StorageResult<()>;

    /// Export all data
    async fn export(&self) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Import data (replacing existing data)
    async fn import(
        &self,
        data: Vec<(StorageValue, StorageValue)>,
    ) -> StorageResult<()>;

    /// Get backend-specific configuration
    fn get_config(&self) -> StorageResult<StorageBackendConfig>;

    /// Update backend-specific configuration
    async fn update_config(
        &self,
        config: StorageBackendConfig,
    ) -> StorageResult<()>;
}

/// Storage snapshot for backup/restore operations
#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    pub timestamp: std::time::SystemTime,
    pub data: Vec<u8>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Backend-specific configuration
#[derive(Debug, Clone)]
pub struct StorageBackendConfig {
    pub backend_type: StorageBackendType,
    pub properties: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_display() {
        assert_eq!(StorageBackendType::Memory.to_string(), "memory");
        assert_eq!(StorageBackendType::Redb.to_string(), "redb");
        assert_eq!(StorageBackendType::Fjall.to_string(), "fjall");
        assert_eq!(StorageBackendType::RocksDb.to_string(), "rocksdb");
    }

    #[test]
    fn test_batch_operation() {
        let key = StorageValue::from("test_key");
        let value = StorageValue::from("test_value");

        let put_op = BatchOperation::Put {
            key: key.clone(),
            value: value.clone(),
        };
        let delete_op = BatchOperation::Delete { key };

        match put_op {
            BatchOperation::Put { key: _, value: _ } => (),
            _ => panic!("Expected Put operation"),
        }

        match delete_op {
            BatchOperation::Delete { key: _ } => (),
            _ => panic!("Expected Delete operation"),
        }
    }
}

// ============================================================================
// Specialized Storage Traits for Enhanced Storage Architecture
// ============================================================================

use serde::{Deserialize, Serialize};
use std::error::Error;
use std::time::{Duration, SystemTime};

/// Specialized trait for Raft log storage - append-only, high-throughput writes
#[async_trait]
pub trait RaftLogStorage: Send + Sync + Clone {
    type Error: Error + Send + Sync + 'static;

    /// Append a single log entry at the specified index
    async fn append_entry(
        &self,
        index: u64,
        term: u64,
        entry: &[u8],
    ) -> Result<(), Self::Error>;

    /// Append multiple log entries atomically (batch operation)
    async fn append_entries(
        &self,
        entries: Vec<RaftLogEntry>,
    ) -> Result<(), Self::Error>;

    /// Get a log entry by index
    async fn get_entry(
        &self,
        index: u64,
    ) -> Result<Option<RaftLogEntry>, Self::Error>;

    /// Get a range of log entries efficiently
    async fn get_entries(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<RaftLogEntry>, Self::Error>;

    /// Get the last log index (highest index stored)
    async fn last_index(&self) -> Result<Option<u64>, Self::Error>;

    /// Get the first log index (lowest index stored, after compaction)
    async fn first_index(&self) -> Result<Option<u64>, Self::Error>;

    /// Truncate log entries from index onwards (for log repair)
    async fn truncate_from(&self, index: u64) -> Result<(), Self::Error>;

    /// Truncate log entries before index (for log compaction)
    async fn truncate_before(&self, index: u64) -> Result<(), Self::Error>;

    /// Get log storage statistics and health information
    async fn log_stats(&self) -> Result<RaftLogStats, Self::Error>;

    /// Force flush any buffered writes to durable storage
    async fn sync(&self) -> Result<(), Self::Error>;
}

/// Raft log entry with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaftLogEntry {
    pub index: u64,
    pub term: u64,
    pub entry_type: RaftLogEntryType,
    pub data: Vec<u8>,
    pub timestamp: SystemTime,
    pub checksum: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RaftLogEntryType {
    Normal,        // Regular application command
    Configuration, // Membership change
    Snapshot,      // Snapshot marker
    NoOp,          // No-operation (for leader election)
}

#[derive(Debug, Clone)]
pub struct RaftLogStats {
    pub first_index: Option<u64>,
    pub last_index: Option<u64>,
    pub entry_count: u64,
    pub total_size_bytes: u64,
    pub compacted_entries: u64,
    pub write_throughput_ops_per_sec: f64,
    pub avg_entry_size_bytes: f64,
}

/// Specialized trait for Raft snapshot storage - bulk operations, compression
#[async_trait]
pub trait RaftSnapshotStorage: Send + Sync + Clone {
    type Error: Error + Send + Sync + 'static;

    /// Create a new snapshot with automatic compression
    async fn create_snapshot(
        &self,
        snapshot: RaftSnapshot,
    ) -> Result<String, Self::Error>;

    /// Get a snapshot by ID
    async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<RaftSnapshot>, Self::Error>;

    /// List available snapshots with metadata
    async fn list_snapshots(
        &self,
    ) -> Result<Vec<SnapshotMetadata>, Self::Error>;

    /// Delete a snapshot by ID
    async fn delete_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<(), Self::Error>;

    /// Stream snapshot data for large snapshots (memory efficient)
    async fn stream_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<SnapshotStream, Self::Error>;

    /// Install snapshot from stream (for network transfer)
    async fn install_snapshot(
        &self,
        stream: SnapshotStream,
    ) -> Result<String, Self::Error>;

    /// Get the latest snapshot metadata
    async fn latest_snapshot(
        &self,
    ) -> Result<Option<SnapshotMetadata>, Self::Error>;

    /// Cleanup old snapshots based on retention policy
    async fn cleanup_snapshots(
        &self,
        retention: SnapshotRetentionPolicy,
    ) -> Result<u64, Self::Error>;

    /// Verify snapshot integrity (checksum validation)
    async fn verify_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<bool, Self::Error>;
}

/// Raft snapshot with compression and metadata
#[derive(Debug, Clone)]
pub struct RaftSnapshot {
    pub id: String,
    pub last_included_index: u64,
    pub last_included_term: u64,
    pub membership_config: Vec<u8>,
    pub data: Vec<u8>,
    pub metadata: SnapshotMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub id: String,
    pub last_included_index: u64,
    pub last_included_term: u64,
    pub size_bytes: u64,
    pub compressed_size_bytes: u64,
    pub created_at: SystemTime,
    pub compression: CompressionType,
    pub checksum: String,
    pub entry_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Gzip,
    Lz4,
    Zstd,
}

#[derive(Debug, Clone)]
pub struct SnapshotRetentionPolicy {
    pub strategy: RetentionStrategy,
    pub keep_count: usize,
    pub max_age: Duration,
    pub max_total_size: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum RetentionStrategy {
    KeepLatest,   // Keep N most recent snapshots
    KeepByAge,    // Keep snapshots newer than max_age
    Combined,     // Keep N recent AND younger than max_age
    SizeBasedLru, // Keep snapshots within total size limit
}

/// Snapshot stream for memory-efficient large snapshot handling
pub struct SnapshotStream {
    pub metadata: SnapshotMetadata,
    pub data: Vec<u8>, // Simplified to avoid external dependencies
}

/// Application data storage - full-featured key-value with transactions
#[async_trait]
pub trait ApplicationDataStorage: StorageBackend {
    type ReadTransaction: ApplicationReadTransaction<Error = StorageError>;
    type WriteTransaction: ApplicationWriteTransaction<Error = StorageError>;

    /// Begin a read-only transaction (potentially more efficient)
    async fn begin_read_transaction(
        &self,
    ) -> Result<Self::ReadTransaction, StorageError>;

    /// Begin a read-write transaction
    async fn begin_write_transaction(
        &self,
    ) -> Result<Self::WriteTransaction, StorageError>;

    /// Range scan with pagination support
    async fn scan_range_paginated(
        &self,
        start: &[u8],
        end: &[u8],
        limit: Option<usize>,
    ) -> Result<
        (Vec<(StorageValue, StorageValue)>, Option<StorageValue>),
        StorageError,
    >;

    /// Multi-get operation for batch reads
    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, StorageError>;

    /// Conditional put operation (compare-and-swap)
    async fn compare_and_swap(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, StorageError>;

    /// Atomic increment operation
    async fn increment(
        &self,
        key: &[u8],
        delta: i64,
    ) -> Result<i64, StorageError>;

    /// Put with time-to-live support
    async fn put_with_ttl(
        &self,
        key: &[u8],
        value: StorageValue,
        ttl: Duration,
    ) -> Result<(), StorageError>;

    /// Create secondary index for efficient queries
    async fn create_index(
        &self,
        index_name: &str,
        key_extractor: IndexKeyExtractor,
    ) -> Result<(), StorageError>;

    /// Query using secondary index
    async fn query_index(
        &self,
        index_name: &str,
        query: IndexQuery,
    ) -> Result<Vec<(StorageValue, StorageValue)>, StorageError>;

    /// Export all data (for snapshot creation)
    async fn export_all(
        &self,
    ) -> Result<Vec<(StorageValue, StorageValue)>, StorageError>;

    /// Import data (for snapshot restoration)
    async fn import_all(
        &self,
        data: Vec<(StorageValue, StorageValue)>,
    ) -> Result<(), StorageError>;
}

/// Read-only transaction interface
#[async_trait]
pub trait ApplicationReadTransaction: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    async fn get(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, Self::Error>;
    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, Self::Error>;
    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error>;
    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error>;
}

/// Read-write transaction interface
#[async_trait]
pub trait ApplicationWriteTransaction: ApplicationReadTransaction {
    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> Result<(), Self::Error>;
    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error>;
    async fn compare_and_swap(
        &mut self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, Self::Error>;
    async fn commit(self) -> Result<(), Self::Error>;
    async fn rollback(self) -> Result<(), Self::Error>;
}

/// Index key extractor function type
pub type IndexKeyExtractor =
    Box<dyn Fn(&[u8], &StorageValue) -> Vec<Vec<u8>> + Send + Sync>;

/// Query types for secondary indexes
#[derive(Debug, Clone)]
pub enum IndexQuery {
    Exact(Vec<u8>),
    Range { start: Vec<u8>, end: Vec<u8> },
    Prefix(Vec<u8>),
}

/// Enhanced error types for specialized storage
#[derive(Debug, thiserror::Error)]
pub enum SpecializedStorageError {
    #[error("Raft log error: {0}")]
    RaftLog(String),

    #[error("Snapshot error: {0}")]
    Snapshot(String),

    #[error("Application data error: {0}")]
    ApplicationData(String),

    #[error("Index error: {0}")]
    Index(String),

    #[error("Compression error: {0}")]
    Compression(String),

    #[error("Stream error: {0}")]
    Stream(String),

    #[error("Storage backend error: {0}")]
    Backend(#[from] StorageError),
}
