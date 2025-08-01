# Storage Separation Design: Addressing Raft Storage Architecture Flaw

## Problem Statement

The current OPRC-ODGM design has a critical flaw: **all storage operations use a single [`StorageBackend`](commons/oprc-dp-storage/src/traits.rs:6) trait**, but Raft consensus requires fundamentally different storage patterns that cannot be efficiently served by a unified interface.

## Storage Requirements Analysis

### Current Single-Backend Approach
```
┌─────────────────────────────────────────┐
│           Single StorageBackend          │
│  ┌─────────┬─────────────┬─────────────┐ │
│  │ Raft    │ Raft        │ Application │ │
│  │ Logs    │ Snapshots   │ Data        │ │
│  └─────────┴─────────────┴─────────────┘ │
└─────────────────────────────────────────┘
```

**Problems:**
- ❌ Raft logs forced into key-value interface (inefficient)
- ❌ No storage specialization for different access patterns
- ❌ Cannot optimize backends independently
- ❌ Configuration inflexibility

### Proposed Multi-Layer Architecture
```
┌─────────────────────────────────────────────────────────┐
│                Composite Storage                        │
├─────────────────┬─────────────────┬─────────────────────┤
│ RaftLogStorage  │ SnapshotStorage │ ApplicationStorage  │
│                 │                 │                     │
│ - Append-only   │ - Bulk I/O      │ - Random access    │
│ - Sequential    │ - Compression   │ - Transactions     │
│ - High writes   │ - Streaming     │ - Range queries    │
│                 │                 │                     │
│ Backend: Memory │ Backend: Disk   │ Backend: RocksDB   │
│ or Log-file     │ + Compression   │ or Redb            │
└─────────────────┴─────────────────┴─────────────────────┘
```

## Design Principles

### 1. **Storage Separation by Access Pattern**
Each storage type optimized for its specific use case:

| Storage Type | Access Pattern | Optimization Focus |
|--------------|---------------|--------------------|
| **Raft Logs** | Append-only, sequential reads | Write throughput, ordered access |
| **Snapshots** | Infrequent bulk operations | Compression, streaming support |
| **App Data** | Random access, complex queries | ACID properties, indexing |

### 2. **Backend Independence**
Different storage types can use different backends:
- Raft logs → Append-optimized storage (log files, memory buffers)
- Snapshots → Compression-enabled storage (disk with compression)
- Application data → Full-featured database (RocksDB, Redb)

### 3. **Composition and Configuration**
Storage layers compose together while maintaining independent configuration:

```rust
pub struct CompositeStorage<L, S, A> {
    pub log_storage: L,      // RaftLogStorage implementation
    pub snapshot_storage: S, // RaftSnapshotStorage implementation  
    pub app_storage: A,      // ApplicationDataStorage implementation
}
```

## High-Level Architecture

### Storage Layer Traits

```rust
/// Specialized for Raft log operations
trait RaftLogStorage {
    async fn append_entry(&self, index: u64, term: u64, entry: &[u8]);
    async fn append_entries(&self, entries: Vec<(u64, u64, Vec<u8>)>);
    async fn get_entries(&self, start: u64, end: u64) -> Vec<RaftLogEntry>;
    async fn truncate_from(&self, index: u64);
    async fn last_index(&self) -> Option<u64>;
}

/// Specialized for snapshot operations
trait RaftSnapshotStorage {
    async fn create_snapshot(&self, snapshot: RaftSnapshot) -> String;
    async fn get_snapshot(&self, id: &str) -> Option<RaftSnapshot>;
    async fn stream_snapshot(&self, id: &str) -> SnapshotStream;
    async fn cleanup_snapshots(&self, policy: RetentionPolicy);
}

/// Full-featured application data storage
trait ApplicationDataStorage: StorageBackend {
    async fn begin_read_transaction(&self) -> ReadTransaction;
    async fn scan_range(&self, start: &[u8], end: &[u8]) -> Vec<(Key, Value)>;
    async fn create_index(&self, name: &str, extractor: KeyExtractor);
    async fn compare_and_swap(&self, key: &[u8], expected: &[u8], new: &[u8]) -> bool;
}
```

### Integration with Raft Consensus

```rust
/// Raft implementation using separated storage
pub struct RaftReplication<L, S, A> {
    raft: Arc<openraft::Raft<TypeConfig>>,
    storage: CompositeStorage<L, S, A>,
    
    // Each storage layer optimized for its purpose:
    // - L handles log append/truncate operations
    // - S handles snapshot creation/installation
    // - A handles application state mutations
}
```

## Benefits of This Approach

### 1. **Performance Optimization**
- **Raft Logs**: Optimized for append operations and sequential access
- **Snapshots**: Optimized for compression and bulk operations  
- **App Data**: Optimized for random access and complex queries

### 2. **Storage Backend Flexibility**
```yaml
# Example configuration
raft_storage:
  log_backend: "append_log"      # Memory + periodic flush
  snapshot_backend: "compressed_disk"  # Disk with Zstd compression
  app_backend: "rocksdb"         # Full-featured database
```

### 3. **Independent Scaling**
- Log storage can be tuned for high write throughput
- Snapshot storage can use different compression algorithms
- Application storage can have different consistency guarantees

### 4. **Operational Benefits**
- **Monitoring**: Per-layer metrics and health checks
- **Backup**: Different backup strategies per storage type
- **Migration**: Can migrate storage layers independently

## Implementation Strategy

### Phase 1: Define Storage Traits
Create specialized traits for each storage type with their optimal interfaces.

### Phase 2: Implement Storage Backends
Create optimized implementations for each storage type:
- `AppendOnlyLogStorage` for Raft logs
- `CompressedSnapshotStorage` for snapshots
- Enhanced `ApplicationDataStorage` for app data

### Phase 3: Composite Storage Integration
Implement `CompositeStorage` that combines all three storage types.

### Phase 4: Raft Integration
Update Raft implementation to use the separated storage architecture.

### Phase 5: Configuration and Testing
Add configuration support and comprehensive testing.

This design addresses the fundamental storage architecture flaw while maintaining clean separation of concerns and optimal performance for each storage type.