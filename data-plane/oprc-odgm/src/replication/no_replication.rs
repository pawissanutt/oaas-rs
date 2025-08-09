//! No-op replication implementation for single-node deployments

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::SystemTime;

use crate::replication::OperationExtra;

use super::{
    Operation, ReplicationError, ReplicationLayer, ReplicationModel,
    ReplicationResponse, ReplicationStatus, ResponseStatus, ShardRequest,
};

/// No-op replication for single-node deployments that interacts directly with storage
#[derive(Debug, Clone)]
pub struct NoReplication<S: oprc_dp_storage::StorageBackend> {
    storage: S,
    _readiness_tx: tokio::sync::watch::Sender<bool>,
    readiness_rx: tokio::sync::watch::Receiver<bool>,
}

impl<S: oprc_dp_storage::StorageBackend + Default> Default
    for NoReplication<S>
{
    fn default() -> Self {
        Self::new(S::default())
    }
}

impl<S: oprc_dp_storage::StorageBackend> NoReplication<S> {
    pub fn new(storage: S) -> Self {
        let (readiness_tx, readiness_rx) = tokio::sync::watch::channel(true);
        Self {
            storage,
            _readiness_tx: readiness_tx,
            readiness_rx,
        }
    }
}

#[async_trait]
impl<S: oprc_dp_storage::StorageBackend + Send + Sync> ReplicationLayer
    for NoReplication<S>
{
    type Error = ReplicationError;

    fn replication_model(&self) -> ReplicationModel {
        ReplicationModel::None
    }

    async fn replicate_write(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, Self::Error> {
        match request.operation {
            Operation::Write(write_op) => {
                let key_bytes = write_op.key.as_slice();

                if write_op.return_old {
                    self.storage
                        .put_with_return(key_bytes, write_op.value)
                        .await
                        .map_err(ReplicationError::StorageError)
                        .and_then(|old_value| {
                            let overwrite = old_value.is_some();
                            Ok(ReplicationResponse {
                                status: ResponseStatus::Applied,
                                data: old_value,
                                extra: OperationExtra::Write(overwrite),
                                ..Default::default()
                            })
                        })
                } else {
                    self.storage
                        .put(key_bytes, write_op.value)
                        .await
                        .map_err(ReplicationError::StorageError)
                        .and_then(|ovr| {
                            Ok(ReplicationResponse {
                                status: ResponseStatus::Applied,
                                extra: OperationExtra::Write(ovr),
                                ..Default::default()
                            })
                        })
                }
            }
            Operation::Delete(delete_op) => {
                let key_bytes = delete_op.key.as_slice();

                self.storage
                    .delete(key_bytes)
                    .await
                    .map_err(ReplicationError::StorageError)?;

                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    ..Default::default()
                })
            }
            Operation::Read(read_op) => {
                let key_bytes = read_op.key.as_slice();

                let data = self
                    .storage
                    .get(key_bytes)
                    .await
                    .map_err(ReplicationError::StorageError)?;

                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    data,
                    ..Default::default()
                })
            }
            Operation::Batch(operations) => {
                // Handle batch operations by processing each operation sequentially
                for operation in operations {
                    let batch_request = ShardRequest {
                        operation,
                        timestamp: request.timestamp,
                        source_node: request.source_node,
                        request_id: format!("{}-batch", request.request_id),
                    };

                    // Recursively call replicate_write for each operation in the batch
                    self.replicate_write(batch_request).await?;
                }

                Ok(ReplicationResponse {
                    status: ResponseStatus::Applied,
                    ..Default::default()
                })
            }
        }
    }

    async fn replicate_read(
        &self,
        request: ShardRequest,
    ) -> Result<ReplicationResponse, Self::Error> {
        // For NoReplication, reads can be handled the same way as writes
        // since there's no distinction between read and write operations
        self.replicate_write(request).await
    }

    async fn add_replica(
        &self,
        _node_id: u64,
        _address: String,
    ) -> Result<(), Self::Error> {
        Err(ReplicationError::UnsupportedOperation(
            "No replication configured".to_string(),
        ))
    }

    async fn remove_replica(&self, _node_id: u64) -> Result<(), Self::Error> {
        Err(ReplicationError::UnsupportedOperation(
            "No replication configured".to_string(),
        ))
    }

    async fn get_replication_status(
        &self,
    ) -> Result<ReplicationStatus, Self::Error> {
        Ok(ReplicationStatus {
            model: ReplicationModel::None,
            healthy_replicas: 1,
            total_replicas: 1,
            lag_ms: None,
            conflicts: 0,
            is_leader: true,
            leader_id: Some(0),
            last_sync: Some(SystemTime::now()),
        })
    }

    async fn sync_replicas(&self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_rx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::{DeleteOperation, ReadOperation, WriteOperation};
    use oprc_dp_storage::{MemoryStorage, StorageValue};

    #[tokio::test]
    async fn test_no_replication() {
        let storage = MemoryStorage::new_with_default().unwrap();
        let replication = NoReplication::new(storage);

        assert_eq!(replication.replication_model(), ReplicationModel::None);

        // Test write operation
        let write_request = ShardRequest {
            operation: Operation::Write(WriteOperation {
                key: StorageValue::from("test"),
                value: StorageValue::from("value"),
                ttl: None,
                return_old: false,
            }),
            timestamp: SystemTime::now(),
            source_node: 1,
            request_id: "test-123".to_string(),
        };

        let response =
            replication.replicate_write(write_request).await.unwrap();
        assert!(matches!(response.status, ResponseStatus::Applied));

        // Test read operation
        let read_request = ShardRequest {
            operation: Operation::Read(ReadOperation {
                key: StorageValue::from("test"),
            }),
            timestamp: SystemTime::now(),
            source_node: 1,
            request_id: "test-read-123".to_string(),
        };

        let read_response =
            replication.replicate_read(read_request).await.unwrap();
        assert!(matches!(read_response.status, ResponseStatus::Applied));
        assert!(read_response.data.is_some());

        // Test delete operation
        let delete_request = ShardRequest {
            operation: Operation::Delete(DeleteOperation {
                key: StorageValue::from("test"),
            }),
            timestamp: SystemTime::now(),
            source_node: 1,
            request_id: "test-delete-123".to_string(),
        };

        let delete_response =
            replication.replicate_write(delete_request).await.unwrap();
        assert!(matches!(delete_response.status, ResponseStatus::Applied));

        // Test batch operation
        let batch_request = ShardRequest {
            operation: Operation::Batch(vec![
                Operation::Write(WriteOperation {
                    key: StorageValue::from("batch1"),
                    value: StorageValue::from("value1"),
                    ttl: None,
                    return_old: false,
                }),
                Operation::Write(WriteOperation {
                    key: StorageValue::from("batch2"),
                    value: StorageValue::from("value2"),
                    ttl: None,
                    return_old: false,
                }),
            ]),
            timestamp: SystemTime::now(),
            source_node: 1,
            request_id: "test-batch-123".to_string(),
        };

        let batch_response =
            replication.replicate_write(batch_request).await.unwrap();
        assert!(matches!(batch_response.status, ResponseStatus::Applied));

        let status = replication.get_replication_status().await.unwrap();
        assert_eq!(status.healthy_replicas, 1);
        assert_eq!(status.total_replicas, 1);
        assert!(status.is_leader);
    }
}
