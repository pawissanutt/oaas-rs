# Raft Storage Requirements Analysis

## Overview

Raft consensus algorithm has three distinct storage needs that require different optimization strategies. The current single `StorageBackend` approach fails to address these specific requirements.

## Storage Type Breakdown

### 1. Raft Log Storage

**Purpose**: Store the replicated log entries that form the core of Raft consensus.

**Access Patterns**:
- **Write Pattern**: Strictly append-only, sequential writes
- **Read Pattern**: Sequential reads (for replication) and random access (for log queries)
- **Volume**: High write frequency, moderate read frequency
- **Durability**: Critical - log entries must survive node failures

**Performance Requirements**:
```
┌─────────────────────────────────────────────────────────┐
│                    Raft Log Storage                     │
├─────────────────────────────────────────────────────────┤
│ Write Throughput:  > 10,000 entries/sec                │
│ Write Latency:     < 1ms (local), < 10ms (replicated)  │
│ Read Latency:      < 100μs (single entry)              │
│ Sequential Read:   > 50MB/sec (for catch-up)           │
│ Storage Overhead:  < 20% (efficient encoding)          │
└─────────────────────────────────────────────────────────┘
```

**Specific Operations**:
```rust
// Raft log specific operations
async fn append_entry(index: u64, term: u64, data: &[u8]) -> Result<()>;
async fn append_batch(entries: Vec<LogEntry>) -> Result<()>; // Atomic batch
async fn get_entry(index: u64) -> Result<Option<LogEntry>>;
async fn get_entries_range(start: u64, end: u64) -> Result<Vec<LogEntry>>;
async fn truncate_from(index: u64) -> Result<()>;  // Remove entries >= index
async fn truncate_before(index: u64) -> Result<()>; // Remove entries < index
async fn first_index() -> Result<Option<u64>>;
async fn last_index() -> Result<Option<u64>>;
```

**Storage Characteristics**:
- **Immutability**: Log entries never change once written
- **Ordering**: Strict sequential ordering by index
- **Compaction**: Old entries can be safely removed after snapshotting
- **Durability**: fsync() required for critical log entries

### 2. Raft Snapshot Storage

**Purpose**: Store point-in-time snapshots of application state for log compaction.

**Access Patterns**:
- **Write Pattern**: Infrequent large writes (full state dumps)
- **Read Pattern**: Infrequent large reads (state restoration)
- **Volume**: Low frequency, high volume per operation
- **Compression**: Essential for large state snapshots

**Performance Requirements**:
```
┌─────────────────────────────────────────────────────────┐
│                 Raft Snapshot Storage                   │
├─────────────────────────────────────────────────────────┤
│ Write Throughput:  > 100MB/sec (streaming)             │
│ Read Throughput:   > 200MB/sec (restoration)           │
│ Compression Ratio: > 60% (space efficient)             │
│ Create Latency:    < 10sec (for 1GB snapshot)          │
│ Streaming Support: Required (for large snapshots)      │
└─────────────────────────────────────────────────────────┘
```

**Specific Operations**:
```rust
// Snapshot specific operations
async fn create_snapshot(snapshot: RaftSnapshot) -> Result<SnapshotId>;
async fn get_snapshot(id: SnapshotId) -> Result<Option<RaftSnapshot>>;
async fn list_snapshots() -> Result<Vec<SnapshotMetadata>>;
async fn delete_snapshot(id: SnapshotId) -> Result<()>;
async fn stream_snapshot_out(id: SnapshotId) -> Result<SnapshotStream>;
async fn stream_snapshot_in(stream: SnapshotStream) -> Result<SnapshotId>;
async fn cleanup_old_snapshots(retention: RetentionPolicy) -> Result<u64>;
```

**Storage Characteristics**:
- **Compression**: Zstd, LZ4, or Gzip for space efficiency
- **Checksums**: Data integrity verification
- **Metadata**: Creation time, size, included log index/term
- **Retention**: Configurable cleanup policies
- **Streaming**: Support for large snapshots that don't fit in memory

### 3. Application Data Storage

**Purpose**: Store the actual application state that gets modified by committed log entries.

**Access Patterns**:
- **Write Pattern**: Random writes (key-value updates)
- **Read Pattern**: Random reads with range scans
- **Volume**: Moderate frequency, varies by application
- **Consistency**: ACID transactions required

**Performance Requirements**:
```
┌─────────────────────────────────────────────────────────┐
│               Application Data Storage                  │
├─────────────────────────────────────────────────────────┤
│ Read Latency:      < 1ms (cached), < 10ms (disk)       │
│ Write Latency:     < 5ms (including fsync)             │
│ Transaction TPS:   > 1,000 transactions/sec            │
│ Range Scan:        > 10MB/sec                          │
│ Point Queries:     > 10,000 queries/sec                │
└─────────────────────────────────────────────────────────┘
```

**Specific Operations**:
```rust
// Application data specific operations (extends base StorageBackend)
async fn begin_transaction() -> Result<Transaction>;
async fn begin_read_transaction() -> Result<ReadTransaction>;
async fn get_range(start: &[u8], end: &[u8]) -> Result<Vec<(Key, Value)>>;
async fn scan_prefix(prefix: &[u8]) -> Result<Vec<(Key, Value)>>;
async fn compare_and_swap(key: &[u8], old: Option<&[u8]>, new: &[u8]) -> Result<bool>;
async fn multi_get(keys: Vec<&[u8]>) -> Result<Vec<Option<Value>>>;
async fn create_index(name: &str, key_fn: KeyExtractor) -> Result<()>;
async fn query_index(name: &str, query: IndexQuery) -> Result<Vec<(Key, Value)>>;
```

**Storage Characteristics**:
- **ACID Properties**: Full transaction support
- **Indexing**: Secondary indexes for complex queries
- **Compaction**: Background compaction for space reclamation
- **Caching**: Memory caches for hot data
- **Durability**: Configurable sync policies

## Why Current Single Backend Fails

### Problem Analysis

The current [`StorageBackend`](commons/oprc-dp-storage/src/traits.rs:6) trait provides:
```rust
trait StorageBackend {
    async fn get(key: &[u8]) -> Option<StorageValue>;
    async fn put(key: &[u8], value: StorageValue);
    async fn delete(key: &[u8]);
    async fn scan(prefix: &[u8]) -> Vec<(StorageValue, StorageValue)>;
}
```

**Issues**:

1. **Log Append Inefficiency**
   ```rust
   // Current approach - inefficient for logs
   storage.put(b"log:00000001", log_entry_bytes).await;
   storage.put(b"log:00000002", log_entry_bytes).await;
   
   // What we need - batch append
   log_storage.append_entries(vec![entry1, entry2]).await;
   ```

2. **No Snapshot Streaming**
   ```rust
   // Current - must load entire snapshot into memory
   let snapshot_bytes = storage.get(b"snapshot:123").await;
   
   // What we need - streaming support
   let stream = snapshot_storage.stream_snapshot("123").await;
   ```

3. **Suboptimal Key Encoding**
   ```rust
   // Current - string-based keys with overhead
   storage.put(b"log:00000001", data).await;
   
   // What we need - efficient integer indexing
   log_storage.append_entry(1, term, data).await;
   ```

4. **Mixed Performance Characteristics**
   - Raft logs need write optimization
   - Snapshots need compression
   - App data needs transaction support
   - Single backend can't optimize for all

## Storage Implementation Strategy

### Multi-Backend Composition

```rust
pub struct CompositeRaftStorage<L, S, A> {
    pub log_storage: L,      // Optimized for append-only operations
    pub snapshot_storage: S, // Optimized for compression and streaming  
    pub app_storage: A,      // Optimized for ACID transactions
}

impl<L, S, A> CompositeRaftStorage<L, S, A>
where
    L: RaftLogStorage,
    S: RaftSnapshotStorage, 
    A: ApplicationDataStorage,
{
    pub async fn apply_log_entry(&self, index: u64, term: u64, operation: Operation) -> Result<()> {
        // 1. Append to log storage (for replication)
        let entry_data = bincode::serialize(&operation)?;
        self.log_storage.append_entry(index, term, &entry_data).await?;
        
        // 2. Apply to application storage (for state machine)
        match operation {
            Operation::Put { key, value } => {
                self.app_storage.put(&key, value).await?;
            }
            Operation::Delete { key } => {
                self.app_storage.delete(&key).await?;
            }
        }
        
        Ok(())
    }
    
    pub async fn create_snapshot(&self, up_to_index: u64) -> Result<SnapshotId> {
        // 1. Export all application data
        let app_data = self.app_storage.export_all().await?;
        
        // 2. Create compressed snapshot
        let snapshot = RaftSnapshot {
            last_included_index: up_to_index,
            last_included_term: self.get_term_at_index(up_to_index).await?,
            data: app_data,
        };
        
        let snapshot_id = self.snapshot_storage.create_snapshot(snapshot).await?;
        
        // 3. Compact log storage (remove old entries)
        self.log_storage.truncate_before(up_to_index).await?;
        
        Ok(snapshot_id)
    }
}
```

### Backend Selection Examples

```yaml
# High-performance configuration
raft_storage:
  log_backend:
    type: "append_log"
    config:
      buffer_size: 10000
      sync_policy: "batch"  # Group fsync calls
      
  snapshot_backend:
    type: "compressed_file"
    config:
      compression: "zstd"
      compression_level: 3
      
  app_backend:
    type: "rocksdb"
    config:
      cache_size: "1GB"
      compaction_style: "level"

# Memory-optimized configuration  
raft_storage:
  log_backend:
    type: "memory_log"
    config:
      max_entries: 100000
      
  snapshot_backend:
    type: "memory_snapshot"
    
  app_backend:
    type: "memory"
```

This separation allows each storage type to be optimized independently while maintaining clean interfaces and composition flexibility.