use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::watch;

use super::config::{ShardConfig, ShardError, ShardMetrics};
use super::traits::{
    ReplicationType, ShardMetadata, ShardState, ShardTransaction, StorageType,
};
use crate::replication::{
    DeleteOperation, Operation, ReplicationLayer, ShardRequest, WriteOperation,
};
use oprc_dp_storage::{StorageBackend, StorageTransaction, StorageValue};

/// Unified shard implementation that combines storage and replication
pub struct UnifiedShard<S: StorageBackend, R: ReplicationLayer> {
    metadata: ShardMetadata,
    storage: S,
    replication: Option<R>,
    metrics: Arc<ShardMetrics>,
    config: ShardConfig,
    readiness_tx: watch::Sender<bool>,
    readiness_rx: watch::Receiver<bool>,
}

impl<S: StorageBackend, R: ReplicationLayer> UnifiedShard<S, R> {
    /// Create a new unified shard
    pub async fn new(
        metadata: ShardMetadata,
        storage: S,
        replication: Option<R>,
    ) -> Result<Self, ShardError> {
        let metrics = Arc::new(ShardMetrics::new(
            &metadata.collection,
            metadata.partition_id,
        ));
        let config = ShardConfig::from_metadata(&metadata);
        let (readiness_tx, readiness_rx) = watch::channel(false);

        Ok(Self {
            metadata,
            storage,
            replication,
            metrics,
            config,
            readiness_tx,
            readiness_rx,
        })
    }

    /// Initialize the unified shard
    pub async fn initialize(&self) -> Result<(), ShardError> {
        // Storage backend initialization is implicit

        // Initialize replication if configured
        if let Some(_repl) = &self.replication {
            // Replication initialization would go here
        }

        // Mark as ready
        let _ = self.readiness_tx.send(true);
        Ok(())
    }

    /// Execute an operation with replication coordination
    async fn execute_with_replication<T>(
        &self,
        operation: Operation,
        local_fn: impl std::future::Future<Output = Result<T, ShardError>>,
    ) -> Result<T, ShardError> {
        match &self.replication {
            Some(repl) => {
                // Create replication request
                let request = ShardRequest {
                    operation,
                    timestamp: std::time::SystemTime::now(),
                    source_node: self.metadata.id,
                    request_id: uuid::Uuid::new_v4().to_string(),
                };

                // Execute through replication layer
                let _response = repl
                    .replicate_write(request)
                    .await
                    .map_err(|e| ShardError::ReplicationError(e.to_string()))?;

                // Execute locally
                local_fn.await
            }
            None => {
                // No replication, execute locally
                local_fn.await
            }
        }
    }
}

/// Implementation of ShardState trait for UnifiedShard with memory storage
#[async_trait]
impl<S: StorageBackend + 'static, R: ReplicationLayer + 'static> ShardState
    for UnifiedShard<S, R>
{
    type Key = String;
    type Entry = StorageValue;
    type Error = ShardError;

    fn meta(&self) -> &ShardMetadata {
        &self.metadata
    }

    fn storage_type(&self) -> StorageType {
        // For now, categorize based on storage backend type
        match self.metadata.storage_backend_type() {
            oprc_dp_storage::StorageBackendType::Memory => StorageType::Memory,
            _ => StorageType::Persistent,
        }
    }

    fn replication_type(&self) -> ReplicationType {
        self.metadata.replication_type()
    }

    async fn initialize(&self) -> Result<(), Self::Error> {
        // Storage backend initialization is implicit for memory storage

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

        // Storage cleanup happens automatically with memory storage
        // For persistent storage, we would need explicit cleanup

        Ok(())
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, Self::Error> {
        // Increment operation counter
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        match self.storage.get(key.as_bytes()).await {
            Ok(value) => Ok(value),
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
        // Increment operation counter
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let operation = Operation::Write(WriteOperation {
            key: key.clone(),
            value: entry.clone(),
            ttl: None,
        });

        let local_fn = async {
            match self.storage.put(key.as_bytes(), entry).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    self.metrics
                        .errors_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Err(ShardError::StorageError(e))
                }
            }
        };

        self.execute_with_replication(operation, local_fn).await
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), Self::Error> {
        // Increment operation counter
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let operation = Operation::Delete(DeleteOperation { key: key.clone() });

        let local_fn = async {
            match self.storage.delete(key.as_bytes()).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    self.metrics
                        .errors_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Err(ShardError::StorageError(e))
                }
            }
        };

        self.execute_with_replication(operation, local_fn).await
    }

    async fn count(&self) -> Result<u64, Self::Error> {
        // Increment operation counter
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        match self.storage.count().await {
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
        // Increment operation counter
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let prefix_bytes = prefix.map(|p| p.as_bytes()).unwrap_or(b"");
        match self.storage.scan(prefix_bytes).await {
            Ok(results) => {
                // Convert (StorageValue, StorageValue) to (String, StorageValue)
                let converted: Result<Vec<_>, _> = results
                    .into_iter()
                    .map(|(key_val, value_val)| {
                        // Convert key from StorageValue to String
                        let key_bytes = key_val.as_slice();
                        String::from_utf8(key_bytes.to_vec())
                            .map(|key_str| (key_str, value_val))
                            .map_err(|e| {
                                ShardError::SerializationError(format!(
                                    "Invalid UTF-8 key: {}",
                                    e
                                ))
                            })
                    })
                    .collect();

                match converted {
                    Ok(results) => Ok(results),
                    Err(e) => Err(e),
                }
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
        // Increment operation counter
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Convert to batch operations for replication
        let operations: Vec<Operation> = entries
            .iter()
            .map(|(key, value)| {
                Operation::Write(WriteOperation {
                    key: key.clone(),
                    value: value.clone(),
                    ttl: None,
                })
            })
            .collect();

        let batch_operation = Operation::Batch(operations);

        let local_fn = async {
            // Convert String keys to &[u8] and call individual puts for now
            // TODO: Use actual batch operations when available
            for (key, value) in entries {
                if let Err(e) = self.storage.put(key.as_bytes(), value).await {
                    self.metrics
                        .errors_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Err(ShardError::StorageError(e));
                }
            }
            Ok(())
        };

        self.execute_with_replication(batch_operation, local_fn)
            .await
    }

    async fn batch_delete(
        &self,
        keys: Vec<Self::Key>,
    ) -> Result<(), Self::Error> {
        // Increment operation counter
        self.metrics
            .operations_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        // Convert to batch operations for replication
        let operations: Vec<Operation> = keys
            .iter()
            .map(|key| Operation::Delete(DeleteOperation { key: key.clone() }))
            .collect();

        let batch_operation = Operation::Batch(operations);

        let local_fn = async {
            // Call individual deletes for now
            // TODO: Use actual batch operations when available
            for key in keys {
                if let Err(e) = self.storage.delete(key.as_bytes()).await {
                    self.metrics
                        .errors_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return Err(ShardError::StorageError(e));
                }
            }
            Ok(())
        };

        self.execute_with_replication(batch_operation, local_fn)
            .await
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
        match self.storage.begin_transaction().await {
            Ok(storage_tx) => {
                let tx = UnifiedShardTransaction::<S::Transaction, R> {
                    storage_tx: Some(storage_tx),
                    replication: self.replication.clone(),
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

    fn watch_readiness(&self) -> watch::Receiver<bool> {
        self.readiness_rx.clone()
    }
}

/// Transaction implementation for UnifiedShard
pub struct UnifiedShardTransaction<T: StorageTransaction, R: ReplicationLayer> {
    storage_tx: Option<T>,
    replication: Option<R>,
    metadata: ShardMetadata,
    metrics: Arc<ShardMetrics>,
    completed: bool,
}

#[async_trait]
impl<T: StorageTransaction, R: ReplicationLayer> ShardTransaction for UnifiedShardTransaction<T, R> {
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
            Some(tx) => match tx.get(key.as_bytes()).await {
                Ok(value) => Ok(value),
                Err(e) => Err(ShardError::StorageError(e)),
            },
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
            Some(tx) => match tx.put(key.as_bytes(), entry).await {
                Ok(_) => Ok(()),
                Err(e) => Err(ShardError::StorageError(e)),
            },
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
            Some(tx) => match tx.delete(key.as_bytes()).await {
                Ok(_) => Ok(()),
                Err(e) => Err(ShardError::StorageError(e)),
            },
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
                },
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
                },
                Err(e) => Err(ShardError::StorageError(e)),
            },
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }
}
