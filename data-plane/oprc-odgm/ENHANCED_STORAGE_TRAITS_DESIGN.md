# Enhanced Storage Traits Design for Raft Separation

## Overview

This document defines the enhanced storage trait architecture that addresses the Raft storage separation flaw. The design introduces specialized traits for each storage type while maintaining composability and backward compatibility.

## Core Storage Trait Architecture

### 1. Raft Log Storage Trait

**Purpose**: Optimized for append-only log operations with high write throughput.

```rust
/// Specialized trait for Raft log storage - append-only, high-throughput writes
#[async_trait::async_trait]
pub trait RaftLogStorage: Send + Sync + Clone {
    type Error: Error + Send + Sync + 'static;
    
    /// Append a single log entry at the specified index
    async fn append_entry(&self, index: u64, term: u64, entry: &[u8]) -> Result<(), Self::Error>;
    
    /// Append multiple log entries atomically (batch operation)
    async fn append_entries(&self, entries: Vec<RaftLogEntry>) -> Result<(), Self::Error>;
    
    /// Get a log entry by index
    async fn get_entry(&self, index: u64) -> Result<Option<RaftLogEntry>, Self::Error>;
    
    /// Get a range of log entries efficiently
    async fn get_entries(&self, start: u64, end: u64) -> Result<Vec<RaftLogEntry>, Self::Error>;
    
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
    Normal,         // Regular application command
    Configuration,  // Membership change
    Snapshot,       // Snapshot marker
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
```

### 2. Raft Snapshot Storage Trait

**Purpose**: Optimized for bulk operations with compression and streaming support.

```rust
/// Specialized trait for Raft snapshot storage - bulk operations, compression
#[async_trait::async_trait]
pub trait RaftSnapshotStorage: Send + Sync + Clone {
    type Error: Error + Send + Sync + 'static;
    
    /// Create a new snapshot with automatic compression
    async fn create_snapshot(&self, snapshot: RaftSnapshot) -> Result<String, Self::Error>;
    
    /// Get a snapshot by ID
    async fn get_snapshot(&self, snapshot_id: &str) -> Result<Option<RaftSnapshot>, Self::Error>;
    
    /// List available snapshots with metadata
    async fn list_snapshots(&self) -> Result<Vec<SnapshotMetadata>, Self::Error>;
    
    /// Delete a snapshot by ID
    async fn delete_snapshot(&self, snapshot_id: &str) -> Result<(), Self::Error>;
    
    /// Stream snapshot data for large snapshots (memory efficient)
    async fn stream_snapshot(&self, snapshot_id: &str) -> Result<SnapshotStream, Self::Error>;
    
    /// Install snapshot from stream (for network transfer)
    async fn install_snapshot(&self, stream: SnapshotStream) -> Result<String, Self::Error>;
    
    /// Get the latest snapshot metadata
    async fn latest_snapshot(&self) -> Result<Option<SnapshotMetadata>, Self::Error>;
    
    /// Cleanup old snapshots based on retention policy
    async fn cleanup_snapshots(&self, retention: SnapshotRetentionPolicy) -> Result<u64, Self::Error>;
    
    /// Verify snapshot integrity (checksum validation)
    async fn verify_snapshot(&self, snapshot_id: &str) -> Result<bool, Self::Error>;
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
    KeepLatest,           // Keep N most recent snapshots
    KeepByAge,           // Keep snapshots newer than max_age
    Combined,            // Keep N recent AND younger than max_age
    SizeBasedLru,        // Keep snapshots within total size limit
}
```

### 3. Enhanced Application Data Storage Trait

**Purpose**: Full-featured storage with ACID transactions and advanced features.

```rust
/// Application data storage - full-featured key-value with transactions
#[async_trait::async_trait]
pub trait ApplicationDataStorage: StorageBackend {
    type ReadTransaction: ApplicationReadTransaction<Error = Self::Error>;
    type WriteTransaction: ApplicationWriteTransaction<Error = Self::Error>;
    
    /// Begin a read-only transaction (potentially more efficient)
    async fn begin_read_transaction(&self) -> Result<Self::ReadTransaction, Self::Error>;
    
    /// Begin a read-write transaction
    async fn begin_write_transaction(&self) -> Result<Self::WriteTransaction, Self::Error>;
    
    /// Range scan with pagination support
    async fn scan_range(
        &self, 
        start: &[u8], 
        end: &[u8], 
        limit: Option<usize>
    ) -> Result<(Vec<(StorageValue, StorageValue)>, Option<StorageValue>), Self::Error>;
    
    /// Multi-get operation for batch reads
    async fn multi_get(&self, keys: Vec<&[u8]>) -> Result<Vec<Option<StorageValue>>, Self::Error>;
    
    /// Conditional put operation (compare-and-swap)
    async fn compare_and_swap(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, Self::Error>;
    
    /// Atomic increment operation
    async fn increment(&self, key: &[u8], delta: i64) -> Result<i64, Self::Error>;
    
    /// Put with time-to-live support
    async fn put_with_ttl(
        &self,
        key: &[u8],
        value: StorageValue,
        ttl: Duration,
    ) -> Result<(), Self::Error>;
    
    /// Create secondary index for efficient queries
    async fn create_index(&self, index_name: &str, key_extractor: IndexKeyExtractor) -> Result<(), Self::Error>;
    
    /// Query using secondary index
    async fn query_index(
        &self,
        index_name: &str,
        query: IndexQuery,
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error>;
    
    /// Export all data (for snapshot creation)
    async fn export_all(&self) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error>;
    
    /// Import data (for snapshot restoration)
    async fn import_all(&self, data: Vec<(StorageValue, StorageValue)>) -> Result<(), Self::Error>;
}

/// Read-only transaction interface
#[async_trait::async_trait]
pub trait ApplicationReadTransaction: Send + Sync {
    type Error: Error + Send + Sync + 'static;
    
    async fn get(&self, key: &[u8]) -> Result<Option<StorageValue>, Self::Error>;
    async fn multi_get(&self, keys: Vec<&[u8]>) -> Result<Vec<Option<StorageValue>>, Self::Error>;
    async fn scan_range(&self, start: &[u8], end: &[u8]) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error>;
    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error>;
}

/// Read-write transaction interface
#[async_trait::async_trait]
pub trait ApplicationWriteTransaction: ApplicationReadTransaction {
    async fn put(&mut self, key: &[u8], value: StorageValue) -> Result<(), Self::Error>;
    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error>;
    async fn compare_and_swap(&mut self, key: &[u8], expected: Option<&[u8]>, new_value: Option<StorageValue>) -> Result<bool, Self::Error>;
    async fn commit(self) -> Result<(), Self::Error>;
    async fn rollback(self) -> Result<(), Self::Error>;
}
```

## Composite Storage Architecture

### Multi-Layer Storage Composition

```rust
/// Composite storage that combines all three storage types
pub struct CompositeStorage<L, S, A>
where
    L: RaftLogStorage,
    S: RaftSnapshotStorage,
    A: ApplicationDataStorage,
{
    pub log_storage: L,
    pub snapshot_storage: S,
    pub app_storage: A,
    pub config: CompositeStorageConfig,
    pub metrics: Arc<CompositeStorageMetrics>,
}

impl<L, S, A> CompositeStorage<L, S, A>
where
    L: RaftLogStorage + 'static,
    S: RaftSnapshotStorage + 'static,
    A: ApplicationDataStorage + 'static,
{
    /// Create new composite storage
    pub fn new(
        log_storage: L,
        snapshot_storage: S,
        app_storage: A,
        config: CompositeStorageConfig,
    ) -> Self {
        Self {
            log_storage,
            snapshot_storage,
            app_storage,
            config,
            metrics: Arc::new(CompositeStorageMetrics::new()),
        }
    }
    
    /// Get specialized storage layers
    pub fn get_log_storage(&self) -> &L { &self.log_storage }
    pub fn get_snapshot_storage(&self) -> &S { &self.snapshot_storage }
    pub fn get_app_storage(&self) -> &A { &self.app_storage }
    
    /// Unified health check across all storage layers
    pub async fn health_check(&self) -> Result<CompositeStorageHealth, StorageError> {
        let log_health = self.check_log_storage_health().await?;
        let snapshot_health = self.check_snapshot_storage_health().await?;
        let app_health = self.check_app_storage_health().await?;
        
        Ok(CompositeStorageHealth {
            log_storage: log_health,
            snapshot_storage: snapshot_health,
            app_storage: app_health,
            overall_status: self.compute_overall_status(&log_health, &snapshot_health, &app_health),
        })
    }
    
    /// Coordinated snapshot creation across all storage layers
    pub async fn create_coordinated_snapshot(&self, up_to_log_index: u64) -> Result<String, StorageError> {
        // 1. Get the term at the snapshot index
        let log_entry = self.log_storage.get_entry(up_to_log_index).await?
            .ok_or_else(|| StorageError::NotFound(format!("Log entry at index {}", up_to_log_index)))?;
        
        // 2. Export application state
        let app_data = self.app_storage.export_all().await?;
        
        // 3. Create snapshot with all necessary metadata
        let snapshot = RaftSnapshot {
            id: uuid::Uuid::new_v4().to_string(),
            last_included_index: up_to_log_index,
            last_included_term: log_entry.term,
            membership_config: Vec::new(), // Would include actual membership config
            data: bincode::serialize(&app_data)?,
            metadata: SnapshotMetadata {
                id: uuid::Uuid::new_v4().to_string(),
                last_included_index: up_to_log_index,
                last_included_term: log_entry.term,
                size_bytes: 0, // Will be set by snapshot storage
                compressed_size_bytes: 0,
                created_at: SystemTime::now(),
                compression: self.config.snapshot_compression,
                checksum: String::new(), // Will be computed by snapshot storage
                entry_count: app_data.len() as u64,
            },
        };
        
        // 4. Store snapshot
        let snapshot_id = self.snapshot_storage.create_snapshot(snapshot).await?;
        
        // 5. Compact log storage (remove entries before snapshot)
        self.log_storage.truncate_before(up_to_log_index).await?;
        
        // 6. Update metrics
        self.metrics.snapshots_created.fetch_add(1, Ordering::Relaxed);
        
        Ok(snapshot_id)
    }
    
    /// Restore from snapshot across all storage layers
    pub async fn restore_from_snapshot(&self, snapshot_id: &str) -> Result<(), StorageError> {
        // 1. Get snapshot
        let snapshot = self.snapshot_storage.get_snapshot(snapshot_id).await?
            .ok_or_else(|| StorageError::NotFound(format!("Snapshot {}", snapshot_id)))?;
        
        // 2. Deserialize application data
        let app_data: Vec<(StorageValue, StorageValue)> = bincode::deserialize(&snapshot.data)?;
        
        // 3. Clear current application data and import snapshot data
        self.app_storage.import_all(app_data).await?;
        
        // 4. Update log storage state (would need to track last applied index)
        
        // 5. Update metrics
        self.metrics.snapshots_restored.fetch_add(1, Ordering::Relaxed);
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CompositeStorageConfig {
    pub log_config: RaftLogConfig,
    pub snapshot_config: RaftSnapshotConfig,
    pub app_config: ApplicationStorageConfig,
    pub snapshot_compression: CompressionType,
    pub health_check_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct CompositeStorageHealth {
    pub log_storage: StorageHealthStatus,
    pub snapshot_storage: StorageHealthStatus,
    pub app_storage: StorageHealthStatus,
    pub overall_status: StorageHealthStatus,
}

#[derive(Debug, Clone)]
pub enum StorageHealthStatus {
    Healthy,
    Degraded { reason: String },
    Unhealthy { error: String },
}

pub struct CompositeStorageMetrics {
    pub snapshots_created: AtomicU64,
    pub snapshots_restored: AtomicU64,
    pub log_compactions: AtomicU64,
    pub total_operations: AtomicU64,
}
```

## Configuration Examples

### High-Performance Configuration

```yaml
raft_storage:
  log_storage:
    backend_type: "append_log"
    config:
      buffer_size: 10000
      flush_interval_ms: 100
      sync_policy: "batch"
      checksum_validation: true
      
  snapshot_storage:
    backend_type: "compressed_file"
    config:
      compression: "zstd"
      compression_level: 3
      retention:
        strategy: "keep_latest"
        keep_count: 5
        max_age_hours: 168  # 1 week
        
  app_storage:
    backend_type: "rocksdb"
    config:
      cache_size_mb: 1024
      write_buffer_size_mb: 64
      max_write_buffer_number: 3
      compaction_style: "level"
```

### Memory-Optimized Configuration

```yaml
raft_storage:
  log_storage:
    backend_type: "memory_log"
    config:
      max_entries: 100000
      circular_buffer: true
      
  snapshot_storage:
    backend_type: "memory_snapshot"
    config:
      max_snapshots: 3
      
  app_storage:
    backend_type: "memory"
    config:
      max_size_mb: 512
```

This enhanced trait architecture provides the separation needed for optimal Raft performance while maintaining composability and clear interfaces.