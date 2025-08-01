use async_trait::async_trait;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::watch;

use super::config::{ShardError, ShardMetrics};
use super::traits::{
    FlexibleStorage, ReplicationType, ShardMetadata, ShardState,
    ShardTransaction, StorageType,
};
use crate::replication::{
    DeleteOperation, Operation, ReplicationLayer, ResponseStatus, ShardRequest,
    WriteOperation,
};
use oprc_dp_storage::{
    ApplicationDataStorage, RaftLogStorage, StorageTransaction, StorageValue,
};

// ============================================================================
// Universal UnifiedShard Implementation
// ============================================================================

/// Universal shard implementation supporting all replication types
/// Uses FlexibleStorage - log storage is optional (only for Raft)
pub struct UnifiedShard<L, A, R>
where
    L: RaftLogStorage,
    A: ApplicationDataStorage,
    R: ReplicationLayer,
{
    metadata: ShardMetadata,
    storage: FlexibleStorage<L, A>,
    replication: Option<R>,
    metrics: Arc<ShardMetrics>,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
}

impl<L, A, R> UnifiedShard<L, A, R>
where
    L: RaftLogStorage,
    A: ApplicationDataStorage,
    R: ReplicationLayer,
{
    /// Create a new unified shard with Raft storage (log + app)
    pub async fn new_with_raft_storage(
        metadata: ShardMetadata,
        log_storage: L,
        app_storage: A,
        replication: Option<R>,
    ) -> Result<Self, ShardError> {
        let storage =
            FlexibleStorage::new_raft_storage(log_storage, app_storage);
        Self::new_with_storage(metadata, storage, replication).await
    }

    /// Create a new unified shard with app-only storage (no log storage)
    pub async fn new_with_app_storage(
        metadata: ShardMetadata,
        app_storage: A,
        replication: Option<R>,
    ) -> Result<Self, ShardError> {
        let storage = FlexibleStorage::new_app_only_storage(app_storage);
        Self::new_with_storage(metadata, storage, replication).await
    }

    /// Create a new unified shard with flexible storage
    async fn new_with_storage(
        metadata: ShardMetadata,
        storage: FlexibleStorage<L, A>,
        replication: Option<R>,
    ) -> Result<Self, ShardError> {
        let metrics = Arc::new(ShardMetrics::new(
            &metadata.collection,
            metadata.partition_id,
        ));
        let (readiness_tx, readiness_rx) = watch::channel(false);

        Ok(Self {
            metadata,
            storage,
            replication,
            metrics,
            readiness_tx,
            readiness_rx,
        })
    }

    /// Get last applied index (for Raft snapshots)
    async fn get_last_applied_index(&self) -> Result<u64, ShardError> {
        // This would be maintained by the Raft replication layer
        // For now, return 0
        Ok(0)
    }

    /// Create application state snapshot (for non-Raft replication)
    async fn create_app_state_snapshot(&self) -> Result<String, ShardError> {
        self.storage.create_app_state_snapshot().await.map_err(|e| {
            ShardError::StorageError(oprc_dp_storage::StorageError::Backend(
                e.to_string(),
            ))
        })
    }
}

// ============================================================================
// ShardState Implementation for Universal UnifiedShard
// ============================================================================

#[async_trait]
impl<L, A, R> ShardState for UnifiedShard<L, A, R>
where
    L: RaftLogStorage + 'static,
    A: ApplicationDataStorage + 'static,
    R: ReplicationLayer + 'static,
{
    type Key = String;
    type Entry = StorageValue;
    type Error = ShardError;

    // Storage layer access types
    type LogStorage = L;
    type AppStorage = A;

    fn meta(&self) -> &ShardMetadata {
        &self.metadata
    }

    fn storage_type(&self) -> StorageType {
        match self.storage.get_app_storage().backend_type() {
            oprc_dp_storage::StorageBackendType::Memory => StorageType::Memory,
            _ => StorageType::Persistent,
        }
    }

    fn replication_type(&self) -> ReplicationType {
        match &self.replication {
            None => ReplicationType::None,
            Some(_repl) => {
                // Infer from metadata
                self.metadata.replication_type()
            }
        }
    }

    /// Direct storage layer access
    fn get_log_storage(&self) -> Option<&Self::LogStorage> {
        self.storage.get_log_storage()
    }

    fn get_app_storage(&self) -> &Self::AppStorage {
        self.storage.get_app_storage()
    }

    async fn initialize(&self) -> Result<(), Self::Error> {
        // Storage backend initialization is implicit
        // Initialize replication if configured
        if let Some(_repl) = &self.replication {
            // Replication initialization would go here
        }

        // Mark as ready
        let _ = self.readiness_tx.send(true);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        // Mark as not ready
        let _ = self.readiness_tx.send(false);

        // Close application storage
        self.storage
            .get_app_storage()
            .close()
            .await
            .map_err(ShardError::StorageError)
    }

    fn watch_readiness(&self) -> watch::Receiver<bool> {
        self.readiness_rx.clone()
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, Self::Error> {
        // All replication types can read from local app storage
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let key_bytes = key.as_bytes();
        match self.storage.get_app_storage().get(key_bytes).await {
            Ok(Some(data)) => {
                let entry: Self::Entry = bincode::serde::decode_from_slice(
                    data.as_slice(),
                    bincode::config::standard(),
                )
                .map(|(entry, _)| entry)
                .map_err(|e| ShardError::SerializationError(e.to_string()))?;
                Ok(Some(entry))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    async fn set(
        &self,
        key: Self::Key,
        entry: Self::Entry,
    ) -> Result<(), Self::Error> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let value_bytes =
            bincode::serde::encode_to_vec(&entry, bincode::config::standard())
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

                let response = repl
                    .replicate_write(request)
                    .await
                    .map_err(|e| ShardError::ReplicationError(e.to_string()))?;

                match response.status {
                    ResponseStatus::Applied => Ok(()),
                    ResponseStatus::NotLeader { .. } => {
                        Err(ShardError::NotLeader)
                    }
                    ResponseStatus::Failed(reason) => {
                        Err(ShardError::ReplicationError(reason))
                    }
                    _ => Err(ShardError::ReplicationError(
                        "Unknown response".to_string(),
                    )),
                }
            }
            None => {
                // No replication - direct to app storage
                let key_bytes = key.as_bytes();
                self.storage
                    .get_app_storage()
                    .put(key_bytes, StorageValue::from(value_bytes))
                    .await
                    .map_err(|e| {
                        self.metrics
                            .errors_count
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        ShardError::StorageError(e)
                    })
            }
        }
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), Self::Error> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

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

                let response = repl
                    .replicate_write(request)
                    .await
                    .map_err(|e| ShardError::ReplicationError(e.to_string()))?;

                match response.status {
                    ResponseStatus::Applied => Ok(()),
                    ResponseStatus::NotLeader { .. } => {
                        Err(ShardError::NotLeader)
                    }
                    ResponseStatus::Failed(reason) => {
                        Err(ShardError::ReplicationError(reason))
                    }
                    _ => Err(ShardError::ReplicationError(
                        "Unknown response".to_string(),
                    )),
                }
            }
            None => {
                let key_bytes = key.as_bytes();
                self.storage
                    .get_app_storage()
                    .delete(key_bytes)
                    .await
                    .map_err(|e| {
                        self.metrics
                            .errors_count
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        ShardError::StorageError(e)
                    })
            }
        }
    }

    async fn count(&self) -> Result<u64, Self::Error> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        match self.storage.get_app_storage().count().await {
            Ok(count) => Ok(count),
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    async fn scan(
        &self,
        prefix: Option<&Self::Key>,
    ) -> Result<Vec<(Self::Key, Self::Entry)>, Self::Error> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let prefix_bytes = prefix.map(|p| p.as_bytes()).unwrap_or(b"");
        match self.storage.get_app_storage().scan(prefix_bytes).await {
            Ok(results) => {
                let mut converted = Vec::new();
                for (key_val, value_val) in results {
                    let key =
                        String::from_utf8_lossy(key_val.as_slice()).to_string();
                    let entry: Self::Entry = bincode::serde::decode_from_slice(
                        value_val.as_slice(),
                        bincode::config::standard(),
                    )
                    .map(|(entry, _)| entry)
                    .map_err(|e| {
                        ShardError::SerializationError(e.to_string())
                    })?;
                    converted.push((key, entry));
                }
                Ok(converted)
            }
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    async fn batch_set(
        &self,
        entries: Vec<(Self::Key, Self::Entry)>,
    ) -> Result<(), Self::Error> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for (key, entry) in entries {
            self.set(key, entry).await?;
        }
        Ok(())
    }

    async fn batch_delete(
        &self,
        keys: Vec<Self::Key>,
    ) -> Result<(), Self::Error> {
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        for key in keys {
            self.delete(&key).await?;
        }
        Ok(())
    }

    async fn begin_transaction(
        &self,
    ) -> Result<
        Box<
            dyn ShardTransaction<
                Key = Self::Key,
                Entry = Self::Entry,
                Error = Self::Error,
            >,
        >,
        Self::Error,
    > {
        match self.storage.get_app_storage().begin_transaction().await {
            Ok(storage_tx) => {
                let tx = UniversalShardTransaction {
                    storage_tx: Some(storage_tx),
                    metadata: self.metadata.clone(),
                    metrics: self.metrics.clone(),
                    completed: false,
                };
                Ok(Box::new(tx))
            }
            Err(e) => {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Err(ShardError::StorageError(e))
            }
        }
    }

    /// Enhanced storage operations
    async fn create_snapshot(&self) -> Result<String, Self::Error> {
        // Works with all replication types
        match &self.replication {
            Some(_repl) => {
                match self.replication_type() {
                    ReplicationType::Raft => {
                        // For Raft: coordinated snapshot with log compaction
                        let last_applied =
                            self.get_last_applied_index().await?;
                        self.storage
                            .create_coordinated_snapshot(last_applied)
                            .await
                            .map_err(|e| {
                                ShardError::StorageError(
                                    oprc_dp_storage::StorageError::Backend(
                                        e.to_string(),
                                    ),
                                )
                            })
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

    async fn restore_from_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<(), Self::Error> {
        match self.replication_type() {
            ReplicationType::Raft => {
                // For Raft, try to restore from SnapshotCapableStorage if available
                // Otherwise fallback to application storage snapshot
                let snapshot_key = format!("__raft_snapshot_{}", snapshot_id);
                if let Some(snapshot_data) = self
                    .storage
                    .get_app_storage()
                    .get(snapshot_key.as_bytes())
                    .await
                    .map_err(ShardError::StorageError)?
                {
                    // Restore application state from snapshot
                    let app_data: Vec<(StorageValue, StorageValue)> =
                        bincode::serde::decode_from_slice(
                            snapshot_data.as_slice(),
                            bincode::config::standard(),
                        )
                        .map(|(data, _)| data)
                        .map_err(|e| {
                            ShardError::SerializationError(e.to_string())
                        })?;

                    self.storage
                        .get_app_storage()
                        .import_all(app_data)
                        .await
                        .map_err(ShardError::StorageError)
                } else {
                    Err(ShardError::StorageError(
                        oprc_dp_storage::StorageError::NotFound,
                    ))
                }
            }
            _ => {
                // For other types, restore from application storage snapshot
                let snapshot_key = format!("__snapshot_{}", snapshot_id);
                if let Some(snapshot_data) = self
                    .storage
                    .get_app_storage()
                    .get(snapshot_key.as_bytes())
                    .await
                    .map_err(ShardError::StorageError)?
                {
                    let app_data: Vec<(StorageValue, StorageValue)> =
                        bincode::serde::decode_from_slice(
                            snapshot_data.as_slice(),
                            bincode::config::standard(),
                        )
                        .map(|(data, _)| data)
                        .map_err(|e| {
                            ShardError::SerializationError(e.to_string())
                        })?;

                    self.storage
                        .get_app_storage()
                        .import_all(app_data)
                        .await
                        .map_err(ShardError::StorageError)
                } else {
                    Err(ShardError::StorageError(
                        oprc_dp_storage::StorageError::NotFound,
                    ))
                }
            }
        }
    }

    async fn compact_storage(&self) -> Result<(), Self::Error> {
        // Compact application storage
        self.storage
            .get_app_storage()
            .compact()
            .await
            .map_err(ShardError::StorageError)?;

        // If using Raft, also trigger log compaction through replication layer
        if matches!(self.replication_type(), ReplicationType::Raft) {
            tracing::info!("Log compaction would be triggered through Raft consensus layer");
        }

        Ok(())
    }
}

// ============================================================================
// Transaction Implementation
// ============================================================================

/// Transaction implementation for UniversalShard
pub struct UniversalShardTransaction<T: StorageTransaction> {
    storage_tx: Option<T>,
    metadata: ShardMetadata,
    metrics: Arc<ShardMetrics>,
    completed: bool,
}

#[async_trait]
impl<T: StorageTransaction + 'static> ShardTransaction
    for UniversalShardTransaction<T>
{
    type Key = String;
    type Entry = StorageValue;
    type Error = ShardError;

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match &self.storage_tx {
            Some(tx) => {
                let key_bytes = key.as_bytes();
                match tx.get(key_bytes).await {
                    Ok(Some(data)) => Ok(Some(data)),
                    Ok(None) => Ok(None),
                    Err(e) => Err(ShardError::StorageError(e)),
                }
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn set(
        &mut self,
        key: Self::Key,
        entry: Self::Entry,
    ) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match &mut self.storage_tx {
            Some(tx) => {
                let key_bytes = key.as_bytes();
                match tx.put(key_bytes, entry).await {
                    Ok(_) => Ok(()),
                    Err(e) => Err(ShardError::StorageError(e)),
                }
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn delete(&mut self, key: &Self::Key) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match &mut self.storage_tx {
            Some(tx) => {
                let key_bytes = key.as_bytes();
                match tx.delete(key_bytes).await {
                    Ok(_) => Ok(()),
                    Err(e) => Err(ShardError::StorageError(e)),
                }
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn commit(mut self: Box<Self>) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match self.storage_tx.take() {
            Some(tx) => match tx.commit().await {
                Ok(_) => {
                    self.completed = true;
                    Ok(())
                }
                Err(e) => {
                    self.metrics
                        .errors_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Err(ShardError::StorageError(e))
                }
            },
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), Self::Error> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }

        match self.storage_tx.take() {
            Some(tx) => match tx.rollback().await {
                Ok(_) => {
                    self.completed = true;
                    Ok(())
                }
                Err(e) => Err(ShardError::StorageError(e)),
            },
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }
}
