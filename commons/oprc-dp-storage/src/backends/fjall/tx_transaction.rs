use async_trait::async_trait;
use fjall::{
    Guard, Readable, SingleWriterTxDatabase, SingleWriterTxKeyspace,
    SingleWriterWriteTx as FjallWriteTx, UserValue,
};
use std::sync::Arc;

use crate::{
    ApplicationReadTransaction, ApplicationWriteTransaction, StorageError,
    StorageResult, StorageTransaction, StorageValue,
};

/// Fjall transactional storage transaction (uses TxKeyspace on commit)
pub struct FjallTxTransaction<'a> {
    keyspace: Arc<SingleWriterTxKeyspace>,
    write_tx: FjallWriteTx<'a>,
    committed: bool,
    rolled_back: bool,
}

impl<'a> FjallTxTransaction<'a> {
    pub fn new(
        db: &'a SingleWriterTxDatabase,
        keyspace: Arc<SingleWriterTxKeyspace>,
    ) -> StorageResult<Self> {
        let write_tx = db.write_tx();
        Ok(Self {
            keyspace,
            write_tx,
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

#[async_trait(?Send)]
impl<'a> StorageTransaction for FjallTxTransaction<'a> {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        self.check_state()?;

        // Read through the live write transaction to observe uncommitted writes (RYOW)
        let result = self
            .write_tx
            .get(&*self.keyspace, key)
            .map_err(Self::convert_error)?;

        Ok(result.map(|bytes: UserValue| {
            StorageValue::from_slice(bytes.as_ref())
        }))
    }

    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()> {
        self.check_state()?;

        let bytes = value.as_slice();
        self.write_tx.insert(&*self.keyspace, key, bytes);
        Ok(())
    }

    async fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        self.check_state()?;
        self.write_tx.remove(&*self.keyspace, key);
        Ok(())
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        self.check_state()?;

        let exists = self
            .write_tx
            .contains_key(&*self.keyspace, key)
            .map_err(Self::convert_error)?;

        Ok(exists)
    }

    async fn commit(mut self) -> StorageResult<()> {
        self.check_state()?;

        // Commit the native Fjall write transaction
        self.write_tx.commit().map_err(Self::convert_error)?;

        self.committed = true;
        Ok(())
    }

    async fn rollback(mut self) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction(
                "Transaction already committed",
            ));
        }
        // Dropping the write_tx without commit discards changes
        self.rolled_back = true;
        Ok(())
    }
}

// Implement ApplicationReadTransaction for FjallTxTransaction
#[async_trait(?Send)]
impl<'a> ApplicationReadTransaction for FjallTxTransaction<'a> {
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

        let start_bound = std::ops::Bound::Included(start);
        let end_bound = std::ops::Bound::Included(end);

        // Scan through the write transaction to reflect uncommitted changes
        let results: Vec<(StorageValue, StorageValue)> = self
            .write_tx
            .range::<&[u8], _>(&*self.keyspace, (start_bound, end_bound))
            .map(|guard: Guard| {
                let (k, v) = guard.into_inner().map_err(Self::convert_error)?;
                Ok::<(StorageValue, StorageValue), StorageError>((
                    StorageValue::from_slice(k.as_ref()),
                    StorageValue::from_slice(v.as_ref()),
                ))
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        Ok(results)
    }

    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        StorageTransaction::exists(self, key).await
    }
}

// Implement ApplicationWriteTransaction for FjallTxTransaction
#[async_trait(?Send)]
impl<'a> ApplicationWriteTransaction for FjallTxTransaction<'a> {
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
