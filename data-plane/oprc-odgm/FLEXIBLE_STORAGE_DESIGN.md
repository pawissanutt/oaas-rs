# Flexible Storage Design: Unified Storage Architecture for Raft and Non-Raft

## Overview

This design supports both Raft and non-Raft replication types using a unified storage architecture. The key insight is that snapshots should be handled by ApplicationDataStorage (via optional SnapshotCapableStorage trait), eliminating the need for separate snapshot storage.

## Storage Requirements by Replication Type

### 1. Raft Consensus - Two-Layer Storage
**Needs**: Raft logs + application data with optional zero-copy snapshots
```rust
/// Raft-specific composite storage (no separate snapshot storage)
pub struct RaftStorage<L, A>
where
    L: RaftLogStorage,
    A: ApplicationDataStorage, // Can optionally implement SnapshotCapableStorage
{
    pub log_storage: L,
    pub app_storage: A,
}
```

### 2. MST Replication - Application Storage Only
**Needs**: Only application data storage (MST handles conflict resolution internally)
```rust
/// MST uses simple application storage
pub struct MstStorage<A>
where
    A: ApplicationDataStorage, // Can optionally implement SnapshotCapableStorage
{
    pub app_storage: A,
    // MST conflict resolution is algorithmic, not storage-based
}
```

### 3. Basic/Eventual Consistency - Application Storage Only  
**Needs**: Only application data storage with optional ordering
```rust
/// Basic replication uses application storage
pub struct BasicStorage<A>
where
    A: ApplicationDataStorage, // Can optionally implement SnapshotCapableStorage
{
    pub app_storage: A,
    // Optional: lightweight operation sequencing in app storage
}
```

### 4. No Replication - Application Storage Only
**Needs**: Only application data storage
```rust
/// No replication uses simple application storage
pub struct NoReplicationStorage<A>
where
    A: ApplicationDataStorage, // Can optionally implement SnapshotCapableStorage
{
    pub app_storage: A,
}
```

## Zero-Copy Snapshot Support

All storage types can optionally support zero-copy snapshots by implementing the `SnapshotCapableStorage` trait on their ApplicationDataStorage:

```rust
/// Optional trait for zero-copy snapshots (implemented by storage engines)
#[async_trait::async_trait]
pub trait SnapshotCapableStorage: ApplicationDataStorage {
    type FileReference: Send + Sync + Clone + Serialize + for<'de> Deserialize<'de>;
    
    /// Create zero-copy snapshot referencing immutable files
    async fn create_zero_copy_snapshot(&self) -> Result<ZeroCopySnapshot<Self::FileReference>, Self::Error>;
    
    /// Restore from zero-copy snapshot (hard-link immutable files)
    async fn restore_from_snapshot(&self, snapshot: &ZeroCopySnapshot<Self::FileReference>) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZeroCopySnapshot<T> {
    pub snapshot_id: String,
    pub sequence_number: Option<u64>,
    pub last_included_log_index: Option<u64>, // For Raft coordination
    pub immutable_files: Vec<T>, // Engine-specific file references
    pub total_size_bytes: u64,
    pub created_at: SystemTime,
}
```

## Unified ShardState Trait Design

### Trait with Flexible Storage Types

```rust
/// Unified ShardState that adapts to different storage needs
#[async_trait::async_trait]
pub trait ShardState: Send + Sync {
    type Key: Send + Clone + Serialize + for<'de> Deserialize<'de>;
    type Entry: Send + Sync + Default + Serialize + for<'de> Deserialize<'de>;
    type Error: Error + Send + Sync + 'static;
    type Storage: Send + Sync; // Flexible storage type (RaftStorage, MstStorage, etc.)

    /// Metadata and configuration
    fn meta(&self) -> &ShardMetadata;
    fn storage_type(&self) -> StorageType;
    fn replication_type(&self) -> ReplicationType;

    /// Storage access (type depends on replication model)
    fn get_storage(&self) -> &Self::Storage;

    /// Lifecycle management
    async fn initialize(&self) -> Result<(), Self::Error>;
    async fn close(&mut self) -> Result<(), Self::Error>;
    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool>;

    /// Core data operations
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

    /// Snapshot operations (available if storage supports SnapshotCapableStorage)
    async fn create_snapshot(&self) -> Result<String, Self::Error> {
        Err(Self::Error::from(OdgmError::UnsupportedOperation("Snapshots not supported".to_string())))
    }
    
    async fn restore_from_snapshot(&self, _snapshot_id: &str) -> Result<(), Self::Error> {
        Err(Self::Error::from(OdgmError::UnsupportedOperation("Snapshot restoration not supported".to_string())))
    }
    
    /// Storage optimization
    async fn compact_storage(&self) -> Result<(), Self::Error> {
        // Default: compact application storage if available
        Ok(())
    }
}
```

## Replication-Specific Shard Implementations

### 1. Raft Shard with Full Storage Separation

```rust
/// Raft shard using log storage + application storage (with optional zero-copy snapshots)
pub struct RaftShard<L, A>
where
    L: RaftLogStorage,
    A: ApplicationDataStorage,
{
    metadata: ShardMetadata,
    storage: RaftStorage<L, A>,
    raft_replication: RaftReplication<L, A>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
}

#[async_trait::async_trait]
impl<L, A> ShardState for RaftShard<L, A>
where
    L: RaftLogStorage + 'static,
    A: ApplicationDataStorage + 'static,
{
    type Key = u64;
    type Entry = ObjectEntry;
    type Error = ShardError;
    type Storage = RaftStorage<L, A>;

    fn get_storage(&self) -> &Self::Storage { &self.storage }
    
    fn replication_type(&self) -> ReplicationType { ReplicationType::Consensus }

    async fn set(&self, key: Self::Key, entry: Self::Entry) -> Result<(), Self::Error> {
        // Goes through Raft consensus (uses log + app storage)
        let operation = Operation::Write(WriteOperation {
            key: key.to_string(),
            value: StorageValue::from(bincode::serialize(&entry)?),
            ttl: None,
        });

        let request = ShardRequest {
            operation,
            timestamp: SystemTime::now(),
            source_node: self.metadata.id,
            request_id: uuid::Uuid::new_v4().to_string(),
        };

        let response = self.raft_replication.replicate_write(request).await?;
        
        match response.status {
            ResponseStatus::Applied => Ok(()),
            ResponseStatus::NotLeader { .. } => Err(ShardError::NotLeader),
            ResponseStatus::Failed(reason) => Err(ShardError::ReplicationError(reason)),
        
        match response.status {
            ResponseStatus::Applied => Ok(()),
            ResponseStatus::NotLeader { .. } => Err(ShardError::NotLeader),
            ResponseStatus::Failed(reason) => Err(ShardError::ReplicationError(reason)),
            _ => Err(ShardError::ReplicationError("Unknown response".to_string())),
        }
    }

    async fn create_snapshot(&self) -> Result<String, Self::Error> {
        // Try zero-copy snapshot first if supported
        if let Some(snapshot_capable) = self.storage.app_storage.as_any()
            .downcast_ref::<dyn SnapshotCapableStorage>() {
            
            let mut zero_copy_snapshot = snapshot_capable.create_zero_copy_snapshot().await
                .map_err(ShardError::StorageError)?;
            
            // Add Raft coordination info
            let last_applied = self.raft_replication.get_last_applied_index().await?;
            zero_copy_snapshot.last_included_log_index = Some(last_applied);
            
            // Store snapshot metadata in log storage
            let snapshot_id = zero_copy_snapshot.snapshot_id.clone();
            self.storage.log_storage.store_snapshot_metadata(snapshot_id.clone(), zero_copy_snapshot).await
                .map_err(ShardError::StorageError)?;
            
            Ok(snapshot_id)
        } else {
            // Fallback to traditional snapshot
            let app_data = self.storage.app_storage.export_all().await
                .map_err(ShardError::StorageError)?;
            
            let snapshot_data = bincode::serialize(&app_data)?;
            let snapshot_id = uuid::Uuid::new_v4().to_string();
            
            // Store snapshot in application storage with special key
            let snapshot_key = format!("__raft_snapshot_{}", snapshot_id);
            self.storage.app_storage.put(snapshot_key.as_bytes(), StorageValue::from(snapshot_data)).await
                .map_err(ShardError::StorageError)?;
            
            Ok(snapshot_id)
        }
    }

    // Direct access to Raft storage layers
    pub fn get_log_storage(&self) -> &L { &self.storage.log_storage }
    pub fn get_app_storage(&self) -> &A { &self.storage.app_storage }
}
```

### 2. MST Shard with Simple Application Storage

```rust
/// MST shard using only application storage
pub struct MstShard<A>
where
    A: ApplicationDataStorage,
{
    metadata: ShardMetadata,
    storage: MstStorage<A>,
    mst_replication: MstReplication<A>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
}

#[async_trait::async_trait]
impl<A> ShardState for MstShard<A>
where
    A: ApplicationDataStorage + 'static,
{
    type Key = u64;
    type Entry = ObjectEntry;
    type Error = ShardError;
    type Storage = MstStorage<A>;

    fn get_storage(&self) -> &Self::Storage { &self.storage }
    
    fn replication_type(&self) -> ReplicationType { ReplicationType::ConflictFree }

    async fn set(&self, key: Self::Key, entry: Self::Entry) -> Result<(), Self::Error> {
        // MST replication handles conflict resolution algorithmically
        let operation = Operation::Write(WriteOperation {
            key: key.to_string(),
            value: StorageValue::from(bincode::serialize(&entry)?),
            ttl: None,
        });

        let request = ShardRequest {
            operation,
            timestamp: SystemTime::now(),
            source_node: self.metadata.id,
            request_id: uuid::Uuid::new_v4().to_string(),
        };

        // MST replication applies locally first, then replicates
        let response = self.mst_replication.replicate_write(request).await?;
        
        match response.status {
            ResponseStatus::Applied => Ok(()),
            ResponseStatus::Conflict(reason) => {
                // MST handles conflicts automatically, this shouldn't happen
                tracing::warn!("Unexpected MST conflict: {}", reason);
                Ok(())
            },
            ResponseStatus::Failed(reason) => Err(ShardError::ReplicationError(reason)),
            _ => Err(ShardError::ReplicationError("Unknown response".to_string())),
        }
    }

    async fn create_snapshot(&self) -> Result<String, Self::Error> {
        // MST snapshot is just the current application state
        let app_data = self.storage.app_storage.export_all().await
            .map_err(ShardError::StorageError)?;
        
        let snapshot_data = bincode::serialize(&app_data)?;
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        
        // Store in application storage with special key
        let snapshot_key = format!("__snapshot_{}", snapshot_id);
        self.storage.app_storage.put(snapshot_key.as_bytes(), StorageValue::from(snapshot_data)).await
            .map_err(ShardError::StorageError)?;
        
        Ok(snapshot_id)
    }

    // Direct access to application storage
    pub fn get_app_storage(&self) -> &A { &self.storage.app_storage }
}
```

### 3. Basic Shard with Simple Storage

```rust
/// Basic replication shard using only application storage
pub struct BasicShard<A>
where
    A: ApplicationDataStorage,
{
    metadata: ShardMetadata,
    storage: BasicStorage<A>,
    basic_replication: Option<BasicReplication<A>>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
}

#[async_trait::async_trait]
impl<A> ShardState for BasicShard<A>
where
    A: ApplicationDataStorage + 'static,
{
    type Key = u64;
    type Entry = ObjectEntry;
    type Error = ShardError;
    type Storage = BasicStorage<A>;

    fn get_storage(&self) -> &Self::Storage { &self.storage }
    
    fn replication_type(&self) -> ReplicationType { 
        match &self.basic_replication {
            Some(_) => ReplicationType::EventualConsistency,
            None => ReplicationType::None,
        }
    }

    async fn set(&self, key: Self::Key, entry: Self::Entry) -> Result<(), Self::Error> {
        let key_bytes = key.to_be_bytes();
        let value_bytes = bincode::serialize(&entry)?;
        
        match &self.basic_replication {
            Some(repl) => {
                // Apply locally first, then replicate asynchronously
                self.storage.app_storage.put(&key_bytes, StorageValue::from(value_bytes.clone())).await
                    .map_err(ShardError::StorageError)?;
                
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

                // Fire and forget async replication
                tokio::spawn({
                    let repl = repl.clone();
                    async move {
                        if let Err(e) = repl.replicate_write(request).await {
                            tracing::warn!("Async replication failed: {}", e);
                        }
                    }
                });
                
                Ok(())
            }
            None => {
                // No replication - direct storage
                self.storage.app_storage.put(&key_bytes, StorageValue::from(value_bytes)).await
                    .map_err(ShardError::StorageError)
            }
        }
    }

    // Basic snapshot is just application state export
    async fn create_snapshot(&self) -> Result<String, Self::Error> {
        let app_data = self.storage.app_storage.export_all().await
            .map_err(ShardError::StorageError)?;
        
        let snapshot_data = bincode::serialize(&app_data)?;
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        
        let snapshot_key = format!("__snapshot_{}", snapshot_id);
        self.storage.app_storage.put(snapshot_key.as_bytes(), StorageValue::from(snapshot_data)).await
            .map_err(ShardError::StorageError)?;
        
        Ok(snapshot_id)
    }

    pub fn get_app_storage(&self) -> &A { &self.storage.app_storage }
}
```

## Factory Pattern for Different Shard Types

### Shard Type Factory

```rust
pub struct ShardFactory;

impl ShardFactory {
    /// Create Raft shard with two-layer storage (log + app with optional zero-copy snapshots)
    pub async fn create_raft_shard(
        metadata: ShardMetadata,
    ) -> Result<Box<dyn ShardState<Key = u64, Entry = ObjectEntry, Error = ShardError>>, ShardError> {
        let log_storage = AppendOnlyLogStorage::new(MemoryStorage::new()).await?;
        let app_storage = RocksDbSnapshotStorage::new("/tmp/data").await?; // Implements SnapshotCapableStorage
        
        let raft_storage = RaftStorage {
            log_storage,
            app_storage,
        };
        
        let raft_replication = RaftReplication::new(metadata.id, raft_storage.clone()).await?;
        
        let shard = RaftShard {
            metadata,
            storage: raft_storage,
            raft_replication,
            readiness_tx: watch::channel(false).0,
            readiness_rx: watch::channel(false).1,
        };
        
        Ok(Box::new(shard))
    }

    /// Create MST shard with simple application storage
    pub async fn create_mst_shard(
        metadata: ShardMetadata,
    ) -> Result<Box<dyn ShardState<Key = u64, Entry = ObjectEntry, Error = ShardError>>, ShardError> {
        let app_storage = MemoryStorage::new(); // MST often uses memory for speed
        
        let mst_storage = MstStorage { app_storage };
        let mst_replication = MstReplication::new(metadata.id, mst_storage.app_storage.clone()).await?;
        
        let shard = MstShard {
            metadata,
            storage: mst_storage,
            mst_replication,
            readiness_tx: watch::channel(false).0,
            readiness_rx: watch::channel(false).1,
        };
        
        Ok(Box::new(shard))
    }

    /// Create basic/no-replication shard with simple storage
    pub async fn create_basic_shard(
        metadata: ShardMetadata,
        enable_replication: bool,
    ) -> Result<Box<dyn ShardState<Key = u64, Entry = ObjectEntry, Error = ShardError>>, ShardError> {
        let app_storage = MemoryStorage::new(); // Or other ApplicationDataStorage
        
        let basic_storage = BasicStorage { app_storage };
        let basic_replication = if enable_replication {
            Some(BasicReplication::new(metadata.id, basic_storage.app_storage.clone()).await?)
        } else {
            None
        };
        
        let shard = BasicShard {
            metadata,
            storage: basic_storage,
            basic_replication,
            readiness_tx: watch::channel(false).0,
            readiness_rx: watch::channel(false).1,
        };
        
        Ok(Box::new(shard))
    }
}
```

## ObjectShard Integration

### Universal ObjectShard

```rust
/// ObjectShard that can use any ShardState implementation
#[derive(Clone)]
pub struct ObjectShard {
    shard_state: Arc<dyn ShardState<Key = u64, Entry = ObjectEntry, Error = ShardError>>,
    z_session: zenoh::Session,
    // ... other fields remain the same
}

impl ObjectShard {
    /// Create ObjectShard with any shard state implementation
    pub fn new(
        shard_state: Arc<dyn ShardState<Key = u64, Entry = ObjectEntry, Error = ShardError>>,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Self {
        // Same constructor as before, but now works with any ShardState
        Self {
            shard_state,
            z_session,
            // ... initialize other fields
        }
    }

    /// Factory methods for specific shard types
    pub async fn create_raft_shard(
        metadata: ShardMetadata,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<Self, OdgmError> {
        let shard_state = ShardFactory::create_raft_shard(metadata).await?;
        Ok(Self::new(shard_state, z_session, event_manager))
    }

    pub async fn create_mst_shard(
        metadata: ShardMetadata,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<Self, OdgmError> {
        let shard_state = ShardFactory::create_mst_shard(metadata).await?;
        Ok(Self::new(shard_state, z_session, event_manager))
    }

    pub async fn create_basic_shard(
        metadata: ShardMetadata,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
        enable_replication: bool,
    ) -> Result<Self, OdgmError> {
        let shard_state = ShardFactory::create_basic_shard(metadata, enable_replication).await?;
        Ok(Self::new(shard_state, z_session, event_manager))
    }
}
```

## Summary

This unified design provides:

1. **Raft**: Two-layer storage (`RaftStorage<L, A>`) with optional zero-copy snapshots for optimal consensus performance
2. **MST**: Simple application storage (`MstStorage<A>`) - conflict resolution is algorithmic, optional zero-copy snapshots
3. **Basic/None**: Simple application storage (`BasicStorage<A>`) - minimal overhead, optional zero-copy snapshots
4. **Universal Interface**: All shard types implement the same `ShardState` trait
5. **Type Safety**: Each shard type exposes only the storage layers it actually uses
6. **Performance**: Each replication type uses optimal storage architecture for its needs
7. **Zero-Copy Snapshots**: ApplicationDataStorage can optionally implement `SnapshotCapableStorage` for engine-native snapshots
8. **No Unused Storage**: Eliminates separate snapshot storage layer - handled by ApplicationDataStorage when needed

### Key Benefits

- **Simplified Architecture**: No separate snapshot storage layer reduces complexity
- **Engine-Native Snapshots**: Zero-copy snapshots leverage storage engine immutable structures (RocksDB SST files, Redb pages, Fjall segments)
- **Flexible Replication**: Same storage approach works for Raft, MST, Basic, and No-replication scenarios  
- **Type Safety**: Compile-time guarantee that each shard type only uses the storage layers it needs
- **Performance**: Engine-native operations eliminate data copying for snapshots

This design is both simpler and more performant than the three-layer approach!