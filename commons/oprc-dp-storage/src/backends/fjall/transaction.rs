use async_trait::async_trait;
use fjall::PartitionHandle;
use std::sync::Arc;

use crate::{
    ApplicationReadTransaction, ApplicationWriteTransaction, StorageError,
    StorageResult, StorageTransaction, StorageValue,
};

/// Fjall storage transaction
pub struct FjallTransaction {
    partition: Arc<PartitionHandle>,
    operations: Vec<TransactionOperation>,
    committed: bool,
    rolled_back: bool,
}

#[derive(Debug, Clone)]
enum TransactionOperation {
    Put { key: Vec<u8>, value: StorageValue },
    Delete { key: Vec<u8> },
}

impl FjallTransaction {
    pub fn new(partition: Arc<PartitionHandle>) -> StorageResult<Self> {
        Ok(Self {
            partition,
            operations: Vec::new(),
            committed: false,
            rolled_back: false,
        })
    }

    /// Convert Fjall error to StorageError
    fn convert_error(err: fjall::Error) -> StorageError {
        match err {
            fjall::Error::Io(io_err) => StorageError::Io(io_err),
            _ => StorageError::backend(err.to_string()),
        }
    }

    /// Check if transaction is in a valid state for operations
    fn check_state(&self) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction(
                "Transaction already committed",
            ));
        }
        if self.rolled_back {
            return Err(StorageError::transaction(
                "Transaction already rolled back",
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl StorageTransaction for FjallTransaction {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        self.check_state()?;

        // For batched operations, we need to check the batch first
        // but Fjall doesn't provide batch read operations, so we read from partition
        let result = self.partition.get(key).map_err(Self::convert_error)?;

        Ok(result.map(|bytes| StorageValue::from(bytes.to_vec())))
    }

    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()> {
        self.check_state()?;

        self.operations.push(TransactionOperation::Put {
            key: key.to_vec(),
            value,
        });

        Ok(())
    }

    async fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        self.check_state()?;

        self.operations
            .push(TransactionOperation::Delete { key: key.to_vec() });

        Ok(())
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        self.check_state()?;

        let exists = self
            .partition
            .contains_key(key)
            .map_err(Self::convert_error)?;

        Ok(exists)
    }

    async fn commit(mut self) -> StorageResult<()> {
        self.check_state()?;

        // Apply all operations atomically
        // Note: Fjall doesn't support true multi-operation transactions,
        // so we apply operations sequentially
        for operation in &self.operations {
            match operation {
                TransactionOperation::Put { key, value } => {
                    let value_bytes = value.clone().into_vec();
                    self.partition
                        .insert(key, &value_bytes)
                        .map_err(Self::convert_error)?;
                }
                TransactionOperation::Delete { key } => {
                    self.partition.remove(key).map_err(Self::convert_error)?;
                }
            }
        }

        self.committed = true;
        Ok(())
    }

    async fn rollback(mut self) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction(
                "Transaction already committed",
            ));
        }

        // Simply discard all operations
        self.operations.clear();
        self.rolled_back = true;
        Ok(())
    }
}

// Implement ApplicationReadTransaction for FjallTransaction
#[async_trait]
impl ApplicationReadTransaction for FjallTransaction {
    type Error = StorageError;

    async fn get(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, Self::Error> {
        StorageTransaction::get(self, key).await
    }

    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, Self::Error> {
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            let value = StorageTransaction::get(self, key).await?;
            results.push(value);
        }

        Ok(results)
    }

    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error> {
        self.check_state()?;

        let _range = start.to_vec()..=end.to_vec();

        // Convert range bounds to bytes
        let start_bound = std::ops::Bound::Included(start);
        let end_bound = std::ops::Bound::Included(end);

        let results: Vec<(StorageValue, StorageValue)> = self
            .partition
            .range::<&[u8], _>((start_bound, end_bound))
            .map(|result| {
                result.map(|(k, v)| {
                    (
                        StorageValue::from(k.to_vec()),
                        StorageValue::from(v.to_vec()),
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::convert_error)?;

        Ok(results)
    }

    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        StorageTransaction::exists(self, key).await
    }
}

// Implement ApplicationWriteTransaction for FjallTransaction
#[async_trait]
impl ApplicationWriteTransaction for FjallTransaction {
    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> Result<(), Self::Error> {
        StorageTransaction::put(self, key, value).await
    }

    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error> {
        StorageTransaction::delete(self, key).await
    }

    async fn compare_and_swap(
        &mut self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, Self::Error> {
        self.check_state()?;

        let current: Option<StorageValue> =
            StorageTransaction::get(self, key).await?;

        // Check if current value matches expected
        let matches = match (&current, expected) {
            (None, None) => true,
            (Some(current_val), Some(expected_bytes)) => {
                let current_bytes = current_val.clone().into_vec();
                current_bytes == expected_bytes
            }
            _ => false,
        };

        if matches {
            match new_value {
                Some(value) => {
                    ApplicationWriteTransaction::put(self, key, value).await?;
                }
                None => {
                    ApplicationWriteTransaction::delete(self, key).await?;
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn commit(self) -> Result<(), Self::Error> {
        StorageTransaction::commit(self).await
    }

    async fn rollback(self) -> Result<(), Self::Error> {
        StorageTransaction::rollback(self).await
    }
}
