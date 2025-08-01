# Unified Shard Integration with Enhanced Storage Architecture

## Overview

This document demonstrates how the enhanced storage architecture with Raft storage separation integrates with the [`UnifiedShard`](data-plane/oprc-odgm/src/shard/unified/core.rs) implementation to provide optimal performance and flexibility.

## Integration Architecture

### Enhanced UnifiedShard with Multi-Storage Support

```rust
/// Enhanced UnifiedShard supporting separated storage layers
pub struct UnifiedShard<L, S, A, R>
where
    L: RaftLogStorage,
    S: RaftSnapshotStorage, 
    A: ApplicationDataStorage,
    R: ReplicationLayer<Storage = CompositeStorage<L, S, A>>,
{
    metadata: ShardMetadata,
    storage: CompositeStorage<L, S, A>,
    replication: Option<R>,
    metrics: Arc<ShardMetrics>,
    config: ShardConfig,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
}
```

### Storage Backend Factory Pattern

```rust
/// Factory for creating specialized storage combinations
pub struct StorageFactory;

impl StorageFactory {
    /// Create optimized storage for high-performance Raft
    pub async fn create_high_performance_raft_storage() -> Result<
        CompositeStorage<
            AppendOnlyLog<MemoryStorage>,
            CompressedFileSnapshots,
            RocksDbStorage
        >, 
        StorageError
    > {
        let log_storage = AppendOnlyLog::new(
            MemoryStorage::new(),
            AppendOnlyLogConfig {
                buffer_size: 10000,
                flush_interval: Duration::from_millis(50),
                sync_writes: false,
                checksum_validation: true,
            }
        ).await?;
        
        let snapshot_storage = CompressedFileSnapshots::new(
            "/var/lib/oprc/snapshots",
            SnapshotStorageConfig {
                compression: CompressionType::Zstd,
                max_snapshot_size: 10 * 1024 * 1024 * 1024, // 10GB
                retention_policy: SnapshotRetentionPolicy {
                    strategy: RetentionStrategy::KeepLatest,
                    keep_count: 5,
                    max_age: Duration::from_secs(7 * 24 * 60 * 60), // 1 week
                    max_total_size: Some(50 * 1024 * 1024 * 1024), // 50GB
                },
            }
        ).await?;
        
        let app_storage = RocksDbStorage::new(
            "/var/lib/oprc/data",
            RocksDbConfig {
                cache_size_mb: 1024,
                write_buffer_size_mb: 128,
                max_write_buffer_number: 4,
                target_file_size_base_mb: 64,
                compression_type: Some(CompressionType::Lz4),
            }
        ).await?;
        
        Ok(CompositeStorage::new(
            log_storage,
            snapshot_storage,
            app_storage,
            CompositeStorageConfig::high_performance()
        ))
    }
    
    /// Create memory-optimized storage for development/testing
    pub async fn create_memory_storage() -> Result<
        CompositeStorage<
            MemoryLogStorage,
            MemorySnapshotStorage,
            MemoryStorage
        >, 
        StorageError
    > {
        let log_storage = MemoryLogStorage::new(100000);
        let snapshot_storage = MemorySnapshotStorage::new(5);
        let app_storage = MemoryStorage::new();
        
        Ok(CompositeStorage::new(
            log_storage,
            snapshot_storage,
            app_storage,
            CompositeStorageConfig::memory_optimized()
        ))
    }
}
```

## Raft Replication Integration

### Enhanced Raft Implementation

```rust
/// Raft replication using separated storage layers
pub struct RaftReplication<L, S, A>
where
    L: RaftLogStorage + 'static,
    S: RaftSnapshotStorage + 'static,
    A: ApplicationDataStorage + 'static,
{
    raft: Arc<openraft::Raft<RaftTypeConfig>>,
    storage: CompositeStorage<L, S, A>,
    node_id: u64,
    config: RaftReplicationConfig,
}

impl<L, S, A> RaftReplication<L, S, A>
where
    L: RaftLogStorage + 'static,
    S: RaftSnapshotStorage + 'static,
    A: ApplicationDataStorage + 'static,
{
    /// Apply a client operation through Raft consensus
    pub async fn apply_operation(&self, operation: Operation) -> Result<ApplyResult, ReplicationError> {
        // 1. Create log entry for the operation
        let log_entry = RaftLogEntry {
            index: 0, // Will be assigned by Raft
            term: 0,  // Will be assigned by Raft
            entry_type: RaftLogEntryType::Normal,
            data: bincode::serialize(&operation)?,
            timestamp: SystemTime::now(),
            checksum: None,
        };
        
        // 2. Propose through Raft (this will replicate to majority)
        let response = self.raft.client_write(operation).await?;
        
        // 3. Once committed, operation is automatically applied to application storage
        // through our RaftStateMachine implementation
        
        Ok(ApplyResult {
            index: response.log_id.index,
            data: response.data,
        })
    }
    
    /// Trigger log compaction with snapshot creation
    pub async fn compact_logs(&self, up_to_index: u64) -> Result<String, ReplicationError> {
        // Use the composite storage's coordinated snapshot functionality
        let snapshot_id = self.storage.create_coordinated_snapshot(up_to_index).await?;
        
        tracing::info!(
            "Created snapshot {} and compacted logs up to index {}",
            snapshot_id,
            up_to_index
        );
        
        Ok(snapshot_id)
    }
}

/// State machine that bridges Raft with our storage layers
pub struct RaftStateMachine<S, A>
where
    S: RaftSnapshotStorage,
    A: ApplicationDataStorage,
{
    snapshot_storage: S,
    app_storage: A,
    last_applied_log: Arc<AtomicU64>,
}

impl<S, A> RaftStateMachine<S, A>
where
    S: RaftSnapshotStorage + 'static,
    A: ApplicationDataStorage + 'static,
{
    /// Apply a committed log entry to application storage
    async fn apply_log_entry(&mut self, entry: RaftLogEntry) -> Result<Option<StorageValue>, RaftError> {
        let operation: Operation = bincode::deserialize(&entry.data)?;
        
        let result = match operation {
            Operation::Write(write_op) => {
                self.app_storage.put(write_op.key.as_bytes(), write_op.value.clone()).await?;
                Some(write_op.value)
            }
            Operation::Delete(delete_op) => {
                self.app_storage.delete(delete_op.key.as_bytes()).await?;
                None
            }
            Operation::Read(_) => {
                // Reads don't modify state, shouldn't be in log
                None
            }
            Operation::Batch(batch_ops) => {
                let mut tx = self.app_storage.begin_write_transaction().await?;
                
                for op in batch_ops {
                    match op {
                        Operation::Write(write_op) => {
                            tx.put(write_op.key.as_bytes(), write_op.value).await?;
                        }
                        Operation::Delete(delete_op) => {
                            tx.delete(delete_op.key.as_bytes()).await?;
                        }
                        _ => {} // Skip nested operations
                    }
                }
                
                tx.commit().await?;
                None
            }
        };
        
        // Update last applied index
        self.last_applied_log.store(entry.index, Ordering::Release);
        
        Ok(result)
    }
    
    /// Install snapshot from another node
    async fn install_snapshot(&mut self, snapshot: RaftSnapshot) -> Result<(), RaftError> {
        // 1. Store the snapshot
        let snapshot_id = self.snapshot_storage.create_snapshot(snapshot.clone()).await?;
        
        // 2. Restore application state from snapshot
        let app_data: Vec<(StorageValue, StorageValue)> = bincode::deserialize(&snapshot.data)?;
        self.app_storage.import_all(app_data).await?;
        
        // 3. Update last applied index
        self.last_applied_log.store(snapshot.last_included_index, Ordering::Release);
        
        tracing::info!(
            "Installed snapshot {} with {} entries, last_included_index: {}",
            snapshot_id,
            app_data.len(),
            snapshot.last_included_index
        );
        
        Ok(())
    }
}
```

## Complete Usage Examples

### Example 1: High-Performance Financial System

```rust
use oprc_odgm::shard::unified::UnifiedShard;
use oprc_odgm::replication::RaftReplication;
use oprc_dp_storage::{CompositeStorage, StorageFactory};

/// Financial transaction system requiring strong consistency
pub async fn create_financial_shard() -> Result<FinancialShard, ShardError> {
    // Create high-performance storage optimized for financial workloads
    let storage = StorageFactory::create_high_performance_raft_storage().await?;
    
    // Configure Raft for strong consistency
    let raft_config = RaftReplicationConfig {
        heartbeat_interval_ms: 500,  // Fast heartbeats for low latency
        election_timeout_min_ms: 1000,
        election_timeout_max_ms: 2000,
        snapshot_threshold: 5000,    // Frequent snapshots for faster recovery
        max_append_entries: 100,     // Small batches for low latency
    };
    
    let replication = RaftReplication::new(
        1, // node_id
        storage.clone(),
        raft_config,
        ZenohRaftNetwork::new("financial-cluster").await?,
    ).await?;
    
    let metadata = ShardMetadata {
        id: 1,
        collection: "financial-accounts".to_string(),
        partition_id: 0,
        shard_type: "raft".to_string(),
        storage_config: Some(StorageConfig::HighPerformanceRaft),
        replication_config: Some(ReplicationConfig::StrongConsensus),
        ..Default::default()
    };
    
    let shard = UnifiedShard::new(
        metadata,
        storage,
        Some(replication),
    ).await?;
    
    shard.initialize().await?;
    Ok(shard)
}

/// Execute financial transaction with ACID guarantees
pub async fn transfer_funds(
    shard: &FinancialShard,
    from_account: &str,
    to_account: &str,
    amount: u64,
) -> Result<TransactionResult, TransferError> {
    // Create batch operation for atomic transfer
    let operations = vec![
        Operation::Write(WriteOperation {
            key: format!("account:{}", from_account),
            value: StorageValue::from(format!("balance:-{}", amount)),
            ttl: None,
        }),
        Operation::Write(WriteOperation {
            key: format!("account:{}", to_account), 
            value: StorageValue::from(format!("balance:+{}", amount)),
            ttl: None,
        }),
    ];
    
    let batch_op = Operation::Batch(operations);
    
    // Apply through Raft consensus - guarantees atomicity across replicas
    let result = shard.replication
        .as_ref()
        .unwrap()
        .apply_operation(batch_op)
        .await?;
    
    Ok(TransactionResult {
        transaction_id: result.index,
        committed_at: SystemTime::now(),
    })
}
```

### Example 2: Collaborative Document System

```rust
/// Collaborative document system using conflict-free replication
pub async fn create_collaborative_shard() -> Result<CollaborativeShard, ShardError> {
    // Use memory storage for fast document operations
    let storage = StorageFactory::create_memory_storage().await?;
    
    // Use MST replication for conflict-free collaboration
    let replication = MstReplication::new(
        storage.clone(),
        MstReplicationConfig {
            mst_config: MstConfig::default(),
            peer_addresses: vec!["tcp://peer1:5000".to_string()],
            sync_interval: Duration::from_secs(10),
            max_delta_size: 1024 * 1024, // 1MB
        },
        2, // node_id
    ).await?;
    
    let metadata = ShardMetadata {
        id: 2,
        collection: "documents".to_string(),
        partition_id: 0,
        shard_type: "mst".to_string(),
        storage_config: Some(StorageConfig::Memory),
        replication_config: Some(ReplicationConfig::ConflictFree),
        ..Default::default()
    };
    
    let shard = UnifiedShard::new(
        metadata,
        storage,
        Some(replication),
    ).await?;
    
    shard.initialize().await?;
    Ok(shard)
}

/// Edit document collaboratively
pub async fn edit_document(
    shard: &CollaborativeShard,
    doc_id: &str,
    user_id: &str,
    changes: DocumentChanges,
) -> Result<(), EditError> {
    let operation = Operation::Write(WriteOperation {
        key: format!("doc:{}:changes:{}", doc_id, uuid::Uuid::new_v4()),
        value: StorageValue::from(bincode::serialize(&ChangeSet {
            user_id: user_id.to_string(),
            changes,
            timestamp: SystemTime::now(),
        })?),
        ttl: None,
    });
    
    // MST replication handles conflict resolution automatically
    shard.replication
        .as_ref()
        .unwrap()
        .replicate_write(ShardRequest::new(operation))
        .await?;
    
    Ok(())
}
```

### Example 3: Development/Testing Setup

```rust
/// Simple development setup with in-memory storage
pub async fn create_dev_shard() -> Result<DevShard, ShardError> {
    let storage = StorageFactory::create_memory_storage().await?;
    
    // No replication for development
    let replication = None;
    
    let metadata = ShardMetadata {
        id: 999,
        collection: "dev-testing".to_string(),
        partition_id: 0,
        shard_type: "basic".to_string(),
        storage_config: Some(StorageConfig::Memory),
        replication_config: None,
        ..Default::default()
    };
    
    let shard = UnifiedShard::new(
        metadata,
        storage,
        replication,
    ).await?;
    
    shard.initialize().await?;
    Ok(shard)
}
```

## Migration Strategy from Current Implementation

### Backward Compatibility Adapter

```rust
/// Adapter that makes enhanced storage compatible with existing ObjectShard
pub struct StorageAdapter<L, S, A>
where
    L: RaftLogStorage,
    S: RaftSnapshotStorage,
    A: ApplicationDataStorage,
{
    composite: CompositeStorage<L, S, A>,
}

#[async_trait::async_trait]
impl<L, S, A> StorageBackend for StorageAdapter<L, S, A>
where
    L: RaftLogStorage + 'static,
    S: RaftSnapshotStorage + 'static,
    A: ApplicationDataStorage + 'static,
{
    type Error = StorageError;
    type Transaction = ApplicationTransaction<A::WriteTransaction>;
    
    async fn begin_transaction(&self) -> Result<Self::Transaction, Self::Error> {
        let tx = self.composite.app_storage.begin_write_transaction().await?;
        Ok(ApplicationTransaction::new(tx))
    }
    
    async fn get(&self, key: &[u8]) -> Result<Option<StorageValue>, Self::Error> {
        self.composite.app_storage.get(key).await
    }
    
    async fn put(&self, key: &[u8], value: StorageValue) -> Result<(), Self::Error> {
        self.composite.app_storage.put(key, value).await
    }
    
    // ... other StorageBackend methods delegate to app_storage
}

/// Gradual migration path
pub async fn migrate_existing_shard(
    old_shard: ObjectShard
) -> Result<UnifiedShard<AppendOnlyLog<MemoryStorage>, CompressedFileSnapshots, RocksDbStorage, RaftReplication>, MigrationError> {
    // 1. Export data from old shard
    let exported_data = export_shard_data(&old_shard).await?;
    
    // 2. Create enhanced storage
    let storage = StorageFactory::create_high_performance_raft_storage().await?;
    
    // 3. Import data to new storage
    storage.get_app_storage().import_all(exported_data).await?;
    
    // 4. Create new unified shard
    let enhanced_shard = UnifiedShard::new(
        old_shard.meta().clone(),
        storage,
        None, // Start without replication, add later
    ).await?;
    
    enhanced_shard.initialize().await?;
    Ok(enhanced_shard)
}
```

This integration shows how the enhanced storage architecture seamlessly integrates with the unified shard system while providing optimal performance for different use cases and maintaining backward compatibility.