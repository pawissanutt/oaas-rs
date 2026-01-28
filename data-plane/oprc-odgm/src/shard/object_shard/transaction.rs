//! Transaction adapter for ObjectUnifiedShard

use std::sync::Arc;

use crate::shard::object_trait::UnifiedShardTransaction;
use crate::shard::{ObjectData, ShardError, ShardMetrics};
use oprc_dp_storage::StorageValue;

/// Serialize ObjectEntry to StorageValue
pub(super) fn serialize_object_entry(
    entry: &ObjectData,
) -> Result<StorageValue, ShardError> {
    match postcard::to_allocvec(entry) {
        Ok(bytes) => Ok(StorageValue::from(bytes)),
        Err(e) => Err(ShardError::SerializationError(format!(
            "Failed to serialize ObjectEntry: {}",
            e
        ))),
    }
}

/// Deserialize StorageValue to ObjectEntry
pub(super) fn deserialize_object_entry(
    storage_value: &StorageValue,
) -> Result<ObjectData, ShardError> {
    let bytes = storage_value.as_slice();
    match postcard::from_bytes(bytes) {
        Ok(entry) => Ok(entry),
        Err(e) => Err(ShardError::SerializationError(format!(
            "Failed to deserialize ObjectEntry: {}",
            e
        ))),
    }
}

/// Adapter to wrap application write transactions into UnifiedShardTransaction
pub struct UnifiedShardWriteTxAdapter<T> {
    tx: Option<T>,
    metrics: Arc<ShardMetrics>,
    completed: bool,
}

impl<T> UnifiedShardWriteTxAdapter<T> {
    pub fn new(tx: T, metrics: Arc<ShardMetrics>) -> Self {
        Self {
            tx: Some(tx),
            metrics,
            completed: false,
        }
    }
}

#[async_trait::async_trait(?Send)]
impl<T> UnifiedShardTransaction for UnifiedShardWriteTxAdapter<T>
where
    T: oprc_dp_storage::ApplicationWriteTransaction<
            Error = oprc_dp_storage::StorageError,
        >,
{
    async fn get(&self, key: &u64) -> Result<Option<ObjectData>, ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match &self.tx {
            Some(tx) => {
                let key_bytes = key.to_be_bytes();
                match tx.get(&key_bytes).await {
                    Ok(Some(data)) => {
                        let entry = deserialize_object_entry(&data)?;
                        Ok(Some(entry))
                    }
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
        key: u64,
        entry: ObjectData,
    ) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match &mut self.tx {
            Some(tx) => {
                let key_bytes = key.to_be_bytes();
                let value = serialize_object_entry(&entry)?;
                tx.put(&key_bytes, value)
                    .await
                    .map_err(ShardError::StorageError)
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn delete(&mut self, key: &u64) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match &mut self.tx {
            Some(tx) => {
                let key_bytes = key.to_be_bytes();
                tx.delete(&key_bytes)
                    .await
                    .map_err(ShardError::StorageError)
            }
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn commit(mut self: Box<Self>) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match self.tx.take() {
            Some(tx) => tx.commit().await.map_err(|e| {
                self.metrics
                    .errors_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                ShardError::StorageError(e)
            }),
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }

    async fn rollback(mut self: Box<Self>) -> Result<(), ShardError> {
        if self.completed {
            return Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            ));
        }
        match self.tx.take() {
            Some(tx) => tx.rollback().await.map_err(ShardError::StorageError),
            None => Err(ShardError::TransactionError(
                "Transaction already completed".to_string(),
            )),
        }
    }
}
