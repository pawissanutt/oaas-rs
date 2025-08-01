# Final Integration Design: Enhanced Storage with Updated ShardState

## Overview

This document presents the final integration design that updates the [`ShardState`](data-plane/oprc-odgm/src/shard/mod.rs:40) trait to work directly with the enhanced storage architecture, supporting both Raft and non-Raft replication layers without compatibility overhead.

## 1. Updated ShardState Trait

### Enhanced ShardState with Composite Storage

```rust
/// Updated ShardState trait that works directly with composite storage
#[async_trait::async_trait]
pub trait ShardState: Send + Sync {
    type Key: Send + Clone + Serialize + for<'de> Deserialize<'de>;
    type Entry: Send + Sync + Default + Serialize + for<'de> Deserialize<'de>;
    type Error: Error + Send + Sync + 'static;
    
    /// Storage layer access
    type LogStorage: RaftLogStorage;
    type SnapshotStorage: RaftSnapshotStorage;
    type AppStorage: ApplicationDataStorage;

    /// Metadata and configuration
    fn meta(&self) -> &ShardMetadata;
    fn storage_type(&self) -> StorageType;
    fn replication_type(&self) -> ReplicationType;

    /// Storage layer access
    fn get_log_storage(&self) -> &Self::LogStorage;
    fn get_snapshot_storage(&self) -> &Self::SnapshotStorage;
    fn get_app_storage(&self) -> &Self::AppStorage;

    /// Lifecycle management
    async fn initialize(&self) -> Result<(), Self::Error>;
    async fn close(&mut self) -> Result<(), Self::Error>;
    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool>;

    /// Core data operations (work through replication layer)
    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Entry>, Self::Error>;
    async fn set(&self, key: Self::Key, entry: Self::Entry) -> Result<(), Self::Error>;
    async fn delete(&self, key: &Self::Key) -> Result<(), Self::Error>;
    async fn count(&self) -> Result<u64, Self::Error>;

    /// Enhanced operations
    async fn scan(&self, prefix: Option<&Self::Key>) -> Result<Vec<(Self::Key, Self::Entry)>, Self::Error>;
    async fn batch_set(&self, entries: Vec<(Self::Key, Self::Entry)>) -> Result<(), Self::Error>;
    async fn batch_delete(&self, keys: Vec<Self::Key>) -> Result<(), Self::Error>;

    /// Transaction support
    async fn begin_transaction(&self) -> Result<Box<dyn ShardTransaction<Key = Self::Key, Entry = Self::Entry, Error = Self::Error>>, Self::Error>;

    /// Storage-specific operations
    async fn create_snapshot(&self) -> Result<String, Self::Error>;
    async fn restore_from_snapshot(&self, snapshot_id: &str) -> Result<(), Self::Error>;
    async fn compact_storage(&self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum StorageType {
    Memory,
    Persistent,
    Hybrid,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReplicationType {
    None,
    Consensus,
    ConflictFree,
    EventualConsistency,
}
```

## 2. Unified Implementation for All Replication Types

### Universal UnifiedShard Implementation

```rust
/// Universal shard implementation supporting all replication types
pub struct UnifiedShard<L, S, A, R>
where
    L: RaftLogStorage,
    S: RaftSnapshotStorage,
    A: ApplicationDataStorage,
    R: ReplicationLayer,
{
    metadata: ShardMetadata,
    storage: CompositeStorage<L, S, A>,
    replication: Option<R>,
    metrics: Arc<ShardMetrics>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
}

#[async_trait::async_trait]
impl<L, S, A, R> ShardState for UnifiedShard<L, S, A, R>
where
    L: RaftLogStorage + 'static,
    S: RaftSnapshotStorage + 'static,
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + 'static,
{
    type Key = u64;
    type Entry = ObjectEntry;
    type Error = ShardError;
    type LogStorage = L;
    type SnapshotStorage = S;
    type AppStorage = A;

    fn meta(&self) -> &ShardMetadata { &self.metadata }
    
    fn storage_type(&self) -> StorageType {
        match self.storage.get_app_storage().backend_type() {
            StorageBackendType::Memory => StorageType::Memory,
            _ => StorageType::Persistent,
        }
    }
    
    fn replication_type(&self) -> ReplicationType {
        match &self.replication {
            None => ReplicationType::None,
            Some(repl) => match repl.replication_model() {
                ReplicationModel::Consensus { .. } => ReplicationType::Consensus,
                ReplicationModel::ConflictFree { .. } => ReplicationType::ConflictFree,
                ReplicationModel::EventualConsistency { .. } => ReplicationType::EventualConsistency,
                ReplicationModel::None => ReplicationType::None,
            }
        }
    }

    /// Direct storage layer access
    fn get_log_storage(&self) -> &Self::LogStorage { self.storage.get_log_storage() }
    fn get_snapshot_storage(&self) -> &Self::SnapshotStorage { self.storage.get_snapshot_storage() }
    fn get_app_storage(&self) -> &Self::AppStorage { self.storage.get_app_storage() }

    async fn get(&self, key: &Self::Key) -> Result<Option<Self::Entry>, Self::Error> {
        // All replication types can read from local app storage
        let key_bytes = key.to_be_bytes();
        match self.storage.get_app_storage().get(&key_bytes).await {
            Ok(Some(data)) => {
                let entry: ObjectEntry = bincode::deserialize(data.as_slice())
                    .map_err(|e| ShardError::SerializationError(e.to_string()))?;
                Ok(Some(entry))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(ShardError::StorageError(e)),
        }
    }

    async fn set(&self, key: Self::Key, entry: Self::Entry) -> Result<(), Self::Error> {
        let value_bytes = bincode::serialize(&entry)
            .map_err(|e| ShardError::SerializationError(e.to_string()))?;

        match &self.replication {
            Some(repl) => {
                // Route through replication layer
                let operation = Operation::Write(WriteOperation {
                    key: key.to_string(),
                    value: StorageValue::from(value_bytes),
                    ttl: None,
                });

                let request = ShardRequest {
                    operation,
                    timestamp: SystemTime::now(),
                    source_node: self.metadata.id,
                    request_id: uuid::Uuid::new_v4().to_string(),
                };

                let response = repl.replicate_write(request).await
                    .map_err(|e| ShardError::ReplicationError(e.to_string()))?;

                match response.status {
                    ResponseStatus::Applied => Ok(()),
                    ResponseStatus::NotLeader { .. } => Err(ShardError::NotLeader),
                    ResponseStatus::Failed(reason) => Err(ShardError::ReplicationError(reason)),
                    _ => Err(ShardError::ReplicationError("Unknown response".to_string())),
                }
            }
            None => {
                // No replication - direct to app storage
                let key_bytes = key.to_be_bytes();
                self.storage.get_app_storage().put(&key_bytes, StorageValue::from(value_bytes)).await
                    .map_err(ShardError::StorageError)
            }
        }
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), Self::Error> {
        match &self.replication {
            Some(repl) => {
                let operation = Operation::Delete(DeleteOperation {
                    key: key.to_string(),
                });

                let request = ShardRequest {
                    operation,
                    timestamp: SystemTime::now(),
                    source_node: self.metadata.id,
                    request_id: uuid::Uuid::new_v4().to_string(),
                };

                let response = repl.replicate_write(request).await
                    .map_err(|e| ShardError::ReplicationError(e.to_string()))?;

                match response.status {
                    ResponseStatus::Applied => Ok(()),
                    ResponseStatus::NotLeader { .. } => Err(ShardError::NotLeader),
                    ResponseStatus::Failed(reason) => Err(ShardError::ReplicationError(reason)),
                    _ => Err(ShardError::ReplicationError("Unknown response".to_string())),
                }
            }
            None => {
                let key_bytes = key.to_be_bytes();
                self.storage.get_app_storage().delete(&key_bytes).await
                    .map_err(ShardError::StorageError)
            }
        }
    }

    /// Enhanced storage operations
    async fn create_snapshot(&self) -> Result<String, Self::Error> {
        // Works with all replication types
        match &self.replication {
            Some(repl) => {
                match repl.replication_model() {
                    ReplicationModel::Consensus { .. } => {
                        // For Raft: coordinated snapshot with log compaction
                        let last_applied = self.get_last_applied_index().await?;
                        self.storage.create_coordinated_snapshot(last_applied).await
                            .map_err(|e| ShardError::StorageError(e))
                    }
                    _ => {
                        // For other replication types: simple app state snapshot
                        self.create_app_state_snapshot().await
                    }
                }
            }
            None => {
                // No replication: simple app state snapshot
                self.create_app_state_snapshot().await
            }
        }
    }

    async fn compact_storage(&self) -> Result<(), Self::Error> {
        // Compact application storage
        self.storage.get_app_storage().compact().await
            .map_err(ShardError::StorageError)?;

        // If using Raft, also compact logs
        if matches!(self.replication_type(), ReplicationType::Consensus) {
            // Log compaction would be triggered through Raft
            tracing::info!("Log compaction handled by Raft consensus layer");
        }

        Ok(())
    }
}
```

## 3. Replication Layer Integration

### How Different Replication Types Use the Storage Layers

#### 3.1 Raft Replication (Full Multi-Layer Usage)

```rust
/// Raft replication uses all three storage layers
impl<L, S, A> ReplicationLayer for RaftReplication<L, S, A>
where
    L: RaftLogStorage + 'static,
    S: RaftSnapshotStorage + 'static,
    A: ApplicationDataStorage + 'static,
{
    async fn replicate_write(&self, request: ShardRequest) -> Result<ReplicationResponse, Self::Error> {
        match request.operation {
            Operation::Write(write_op) => {
                // 1. Create Raft log entry
                let log_entry = RaftLogEntry {
                    index: 0, // Assigned by Raft
                    term: 0,  // Assigned by Raft
                    entry_type: RaftLogEntryType::Normal,
                    data: bincode::serialize(&write_op)?,
                    timestamp: SystemTime::now(),
                    checksum: None,
                };

                // 2. Append to log storage (replication happens here)
                let response = self.raft.client_write(log_entry).await?;

                // 3. Once committed, Raft state machine applies to app storage
                // 4. Periodic snapshots use snapshot storage + log compaction

                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data: response.data,
                    metadata: HashMap::new(),
                })
            }
            _ => // Handle other operations
        }
    }
}
```

#### 3.2 MST Replication (App + Log Storage)

```rust
/// MST replication uses app storage + lightweight log for conflict resolution
impl<L, S, A> ReplicationLayer for MstReplication<L, S, A>
where
    L: RaftLogStorage + 'static,  // Used for MST operation log
    S: RaftSnapshotStorage + 'static,  // Minimal usage
    A: ApplicationDataStorage + 'static,  // Primary storage
{
    async fn replicate_write(&self, request: ShardRequest) -> Result<ReplicationResponse, Self::Error> {
        match request.operation {
            Operation::Write(write_op) => {
                // 1. Apply to application storage immediately (conflict-free)
                self.storage.get_app_storage()
                    .put(write_op.key.as_bytes(), write_op.value.clone()).await?;

                // 2. Record MST operation in log storage for peers
                let mst_op = MstOperation {
                    key: write_op.key,
                    value: write_op.value,
                    timestamp: SystemTime::now(),
                    node_id: self.node_id,
                };

                let log_entry = RaftLogEntry {
                    index: self.next_sequence_number(),
                    term: 0, // MST doesn't use terms
                    entry_type: RaftLogEntryType::Normal,
                    data: bincode::serialize(&mst_op)?,
                    timestamp: SystemTime::now(),
                    checksum: None,
                };

                self.storage.get_log_storage().append_entry(
                    log_entry.index,
                    log_entry.term,
                    &log_entry.data
                ).await?;

                // 3. Replicate to peers asynchronously
                self.replicate_to_peers(mst_op).await?;

                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data: Some(write_op.value),
                    metadata: HashMap::new(),
                })
            }
            _ => // Handle other operations
        }
    }
}
```

#### 3.3 Basic Replication (App Storage Only)

```rust
/// Basic replication primarily uses app storage
impl<L, S, A> ReplicationLayer for BasicReplication<L, S, A>
where
    L: RaftLogStorage + 'static,     // Minimal usage for ordering
    S: RaftSnapshotStorage + 'static, // For state transfer
    A: ApplicationDataStorage + 'static, // Primary storage
{
    async fn replicate_write(&self, request: ShardRequest) -> Result<ReplicationResponse, Self::Error> {
        match request.operation {
            Operation::Write(write_op) => {
                // 1. Apply to app storage immediately
                self.storage.get_app_storage()
                    .put(write_op.key.as_bytes(), write_op.value.clone()).await?;

                // 2. Optional: Record in log for ordering (if needed)
                if self.config.maintain_ordering {
                    let log_entry = RaftLogEntry {
                        index: self.next_sequence_number(),
                        term: 0,
                        entry_type: RaftLogEntryType::Normal,
                        data: bincode::serialize(&write_op)?,
                        timestamp: SystemTime::now(),
                        checksum: None,
                    };

                    self.storage.get_log_storage().append_entry(
                        log_entry.index,
                        log_entry.term,
                        &log_entry.data
                    ).await?;
                }

                // 3. Async replication to peers
                self.async_replicate_to_peers(write_op).await;

                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data: Some(write_op.value),
                    metadata: HashMap::new(),
                })
            }
            _ => // Handle other operations
        }
    }
}
```

#### 3.4 No Replication (App Storage Only)

```rust
/// No replication uses only app storage
impl<L, S, A> ReplicationLayer for NoReplication<L, S, A>
where
    L: RaftLogStorage + 'static,     // Unused
    S: RaftSnapshotStorage + 'static, // Used for backups
    A: ApplicationDataStorage + 'static, // Primary storage
{
    async fn replicate_write(&self, request: ShardRequest) -> Result<ReplicationResponse, Self::Error> {
        match request.operation {
            Operation::Write(write_op) => {
                // Direct to app storage
                self.storage.get_app_storage()
                    .put(write_op.key.as_bytes(), write_op.value.clone()).await?;

                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data: Some(write_op.value),
                    metadata: HashMap::new(),
                })
            }
            _ => // Handle other operations
        }
    }
}
```

## 4. ObjectShard Integration

### Updated ObjectShard Using Enhanced ShardState

```rust
/// ObjectShard now uses the enhanced ShardState directly
#[derive(Clone)]
pub struct ObjectShard {
    // Enhanced shard state with composite storage
    shard_state: Arc<dyn ShardState<
        Key = u64, 
        Entry = ObjectEntry,
        LogStorage = Box<dyn RaftLogStorage>,
        SnapshotStorage = Box<dyn RaftSnapshotStorage>,
        AppStorage = Box<dyn ApplicationDataStorage>,
    >>,
    
    // Existing networking and management components
    z_session: zenoh::Session,
    inv_net_manager: Arc<Mutex<InvocationNetworkManager>>,
    inv_offloader: Arc<InvocationOffloader>,
    network: Arc<Mutex<ShardNetwork>>,
    token: CancellationToken,
    liveliness_state: MemberLivelinessState,
    event_manager: Option<Arc<EventManager>>,
}

impl ObjectShard {
    /// Enhanced set operation with direct storage access
    pub async fn set_with_events(&self, key: u64, value: ObjectEntry) -> Result<(), OdgmError> {
        let is_new = !self.shard_state.get(&key).await?.is_some();
        let old_entry = if !is_new {
            self.shard_state.get(&key).await?
        } else {
            None
        };

        // Use enhanced shard state (goes through replication if configured)
        self.shard_state.set(key, value.clone()).await?;

        // Trigger events
        if self.event_manager.is_some() {
            self.trigger_data_events(key, &value, old_entry.as_ref(), is_new).await;
        }

        Ok(())
    }

    /// Direct access to storage layers for advanced operations
    pub async fn create_snapshot(&self) -> Result<String, OdgmError> {
        self.shard_state.create_snapshot().await
            .map_err(|e| OdgmError::StorageError(e.to_string()))
    }

    pub async fn get_storage_stats(&self) -> Result<StorageStats, OdgmError> {
        self.shard_state.get_app_storage().stats().await
            .map_err(|e| OdgmError::StorageError(e.to_string()))
    }
}
```

## 5. Factory Pattern for Different Configurations

### Universal Shard Factory

```rust
pub struct UniversalShardFactory;

impl UniversalShardFactory {
    /// Create Raft-based shard with full multi-layer storage
    pub async fn create_raft_shard(
        metadata: ShardMetadata,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<ObjectShard, OdgmError> {
        let storage = StorageFactory::create_high_performance_raft_storage().await?;
        let replication = RaftReplication::new(metadata.id, storage.clone()).await?;
        
        let unified_shard = UnifiedShard::new(
            metadata,
            storage,
            Some(replication),
        ).await?;

        Ok(ObjectShard::new(
            Arc::new(unified_shard),
            z_session,
            event_manager,
        ))
    }

    /// Create MST-based shard
    pub async fn create_mst_shard(
        metadata: ShardMetadata,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<ObjectShard, OdgmError> {
        let storage = StorageFactory::create_conflict_free_storage().await?;
        let replication = MstReplication::new(metadata.id, storage.clone()).await?;
        
        let unified_shard = UnifiedShard::new(
            metadata,
            storage,
            Some(replication),
        ).await?;

        Ok(ObjectShard::new(
            Arc::new(unified_shard),
            z_session,
            event_manager,
        ))
    }

    /// Create basic replication shard
    pub async fn create_basic_shard(
        metadata: ShardMetadata,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<ObjectShard, OdgmError> {
        let storage = StorageFactory::create_basic_storage().await?;
        let replication = BasicReplication::new(metadata.id, storage.clone()).await?;
        
        let unified_shard = UnifiedShard::new(
            metadata,
            storage,
            Some(replication),
        ).await?;

        Ok(ObjectShard::new(
            Arc::new(unified_shard),
            z_session,
            event_manager,
        ))
    }

    /// Create no-replication shard for development
    pub async fn create_dev_shard(
        metadata: ShardMetadata,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<ObjectShard, OdgmError> {
        let storage = StorageFactory::create_memory_storage().await?;
        
        let unified_shard = UnifiedShard::new(
            metadata,
            storage,
            None, // No replication
        ).await?;

        Ok(ObjectShard::new(
            Arc::new(unified_shard),
            z_session,
            event_manager,
        ))
    }
}
```

## Summary

This final design provides:

1. **Updated ShardState trait** that works directly with composite storage
2. **Universal UnifiedShard** that supports all replication types
3. **Clean integration** without compatibility adapters
4. **Storage layer access** for advanced operations
5. **Flexible usage** - each replication type uses storage layers as needed:
   - **Raft**: Uses all three layers (logs, snapshots, app data)
   - **MST**: Uses logs for operations, app storage for data
   - **Basic**: Uses app storage primarily, logs optionally
   - **None**: Uses only app storage (+ snapshots for backups)

The design is clean, performant, and supports all current replication models while leveraging the enhanced storage architecture optimally.