use async_trait::async_trait;
use fjall::{Config, PersistMode, TxKeyspace, TxPartitionHandle};
use std::ops::RangeBounds;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    atomic_stats::AtomicStats, StorageBackend, StorageBackendType,
    StorageConfig, StorageError, StorageResult, StorageStats, StorageValue,
};

use super::tx_transaction::FjallTxTransaction;

/// Fjall storage backend using transactional keyspace (TxKeyspace)
pub struct FjallTxStorage {
    keyspace: Arc<TxKeyspace>,
    partition: Arc<TxPartitionHandle>,
    config: StorageConfig,
    stats: AtomicStats,
    path: PathBuf,
}

impl Clone for FjallTxStorage {
    fn clone(&self) -> Self {
        Self {
            keyspace: Arc::clone(&self.keyspace),
            partition: Arc::clone(&self.partition),
            config: self.config.clone(),
            stats: self.stats.clone(),
            path: self.path.clone(),
        }
    }
}

impl FjallTxStorage {
    /// Create a new transactional Fjall storage backend
    pub fn new(config: StorageConfig) -> StorageResult<Self> {
        let path = config
            .path
            .as_ref()
            .ok_or_else(|| {
                StorageError::configuration(
                    "Path is required for FjallTx storage".to_string(),
                )
            })?
            .clone();

        // Create Fjall config
        let mut fjall_config = Config::new(&path);

        if let Some(cache_size) = config.cache_size_mb {
            fjall_config =
                fjall_config.cache_size((cache_size * 1024 * 1024) as u64);
        }

        // Open transactional keyspace
        let keyspace = TxKeyspace::open(fjall_config).map_err(|e| {
            StorageError::backend(format!("Failed to open TxKeyspace: {}", e))
        })?;

        // Open default partition
        let partition = keyspace
            .open_partition("default", Default::default())
            .map_err(|e| {
                StorageError::backend(format!(
                    "Failed to open partition: {}",
                    e
                ))
            })?;

        Ok(Self {
            keyspace: Arc::new(keyspace),
            partition: Arc::new(partition),
            config,
            stats: AtomicStats::new(StorageBackendType::Fjall),
            path: path.into(),
        })
    }

    /// Convert Fjall error to StorageError
    pub(crate) fn convert_error(err: fjall::Error) -> StorageError {
        match err {
            fjall::Error::Io(io_err) => StorageError::Io(io_err),
            _ => StorageError::backend(err.to_string()),
        }
    }
}

#[async_trait]
impl StorageBackend for FjallTxStorage {
    type Transaction<'a>
        = FjallTxTransaction<'a>
    where
        Self: 'a;

    fn begin_transaction(&self) -> StorageResult<Self::Transaction<'_>> {
        FjallTxTransaction::new(&self.keyspace, Arc::clone(&self.partition))
    }

    #[inline]
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let result = self.partition.get(key).map_err(Self::convert_error)?;
        Ok(result.map(|bytes| StorageValue::from_slice(&bytes)))
    }

    #[inline]
    async fn put(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<bool> {
        let existed = self
            .partition
            .contains_key(key)
            .map_err(Self::convert_error)?;

        // Perform as a single op via the handle (internally transactional)
        self.partition
            .insert(key, value.as_slice())
            .map_err(Self::convert_error)?;

        self.stats.record_put(key.len(), value.len(), existed);
        Ok(!existed)
    }

    async fn put_with_return(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<Option<StorageValue>> {
        let old_value = self.get(key).await?;

        self.partition
            .insert(key, value.as_slice())
            .map_err(Self::convert_error)?;

        self.stats
            .record_put(key.len(), value.len(), old_value.is_some());
        Ok(old_value)
    }

    #[inline]
    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        let value_size = if let Some(value) =
            self.partition.get(key).map_err(Self::convert_error)?
        {
            value.len()
        } else {
            0
        };

        self.partition.remove(key).map_err(Self::convert_error)?;

        if value_size > 0 {
            self.stats.record_delete(key.len(), value_size);
        }
        Ok(())
    }

    async fn delete_range<R>(&self, range: R) -> StorageResult<u64>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        // Collect keys using a read snapshot
        let start_bound = match range.start_bound() {
            std::ops::Bound::Included(k) => {
                std::ops::Bound::Included(k.as_slice())
            }
            std::ops::Bound::Excluded(k) => {
                std::ops::Bound::Excluded(k.as_slice())
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };
        let end_bound = match range.end_bound() {
            std::ops::Bound::Included(k) => {
                std::ops::Bound::Included(k.as_slice())
            }
            std::ops::Bound::Excluded(k) => {
                std::ops::Bound::Excluded(k.as_slice())
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };

        let rtx = self.keyspace.read_tx();
        let keys_and_sizes: Vec<(Vec<u8>, usize)> = rtx
            .range::<&[u8], _>(&self.partition, (start_bound, end_bound))
            .map(|result| result.map(|(k, v)| (k.to_vec(), k.len() + v.len())))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::convert_error)?;

        let mut count = 0u64;
        let total_size: u64 =
            keys_and_sizes.iter().map(|(_, s)| *s as u64).sum();

        if !keys_and_sizes.is_empty() {
            let mut wtx = self.keyspace.write_tx();
            for (key, _) in &keys_and_sizes {
                wtx.remove(&self.partition, &key[..]);
                count += 1;
            }
            wtx.commit().map_err(Self::convert_error)?;
        }

        if count > 0 {
            self.stats.record_delete_batch(count, total_size);
        }
        Ok(count)
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let exists = self
            .partition
            .contains_key(key)
            .map_err(Self::convert_error)?;
        Ok(exists)
    }

    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        let rtx = self.keyspace.read_tx();
        let results: Vec<(StorageValue, StorageValue)> = rtx
            .prefix(&self.partition, prefix)
            .map(|result| {
                result.map(|(k, v)| {
                    (StorageValue::from_slice(&k), StorageValue::from_slice(&v))
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::convert_error)?;

        Ok(results)
    }

    async fn scan_range<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let start_bound = match range.start_bound() {
            std::ops::Bound::Included(k) => {
                std::ops::Bound::Included(k.as_slice())
            }
            std::ops::Bound::Excluded(k) => {
                std::ops::Bound::Excluded(k.as_slice())
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };
        let end_bound = match range.end_bound() {
            std::ops::Bound::Included(k) => {
                std::ops::Bound::Included(k.as_slice())
            }
            std::ops::Bound::Excluded(k) => {
                std::ops::Bound::Excluded(k.as_slice())
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };

        let rtx = self.keyspace.read_tx();
        let results: Vec<(StorageValue, StorageValue)> = rtx
            .range::<&[u8], _>(&self.partition, (start_bound, end_bound))
            .map(|result| {
                result.map(|(k, v)| {
                    (StorageValue::from_slice(&k), StorageValue::from_slice(&v))
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::convert_error)?;

        Ok(results)
    }

    async fn scan_range_reverse<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let start_bound = match range.start_bound() {
            std::ops::Bound::Included(k) => {
                std::ops::Bound::Included(k.as_slice())
            }
            std::ops::Bound::Excluded(k) => {
                std::ops::Bound::Excluded(k.as_slice())
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };
        let end_bound = match range.end_bound() {
            std::ops::Bound::Included(k) => {
                std::ops::Bound::Included(k.as_slice())
            }
            std::ops::Bound::Excluded(k) => {
                std::ops::Bound::Excluded(k.as_slice())
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };

        let rtx = self.keyspace.read_tx();
        let results: Vec<(StorageValue, StorageValue)> = rtx
            .range::<&[u8], _>(&self.partition, (start_bound, end_bound))
            .rev()
            .map(|result| {
                result.map(|(k, v)| {
                    (StorageValue::from_slice(&k), StorageValue::from_slice(&v))
                })
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::convert_error)?;

        Ok(results)
    }

    async fn get_last(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let result = self
            .partition
            .last_key_value()
            .map_err(Self::convert_error)?
            .map(|(k, v)| {
                (StorageValue::from_slice(&k), StorageValue::from_slice(&v))
            });

        Ok(result)
    }

    async fn get_first(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let result = self
            .partition
            .first_key_value()
            .map_err(Self::convert_error)?
            .map(|(k, v)| {
                (StorageValue::from_slice(&k), StorageValue::from_slice(&v))
            });

        Ok(result)
    }

    async fn count(&self) -> StorageResult<u64> {
        let count = self.partition.approximate_len() as u64;
        Ok(count)
    }

    async fn flush(&self) -> StorageResult<()> {
        // Flush active journal to improve durability
        self.keyspace
            .persist(PersistMode::SyncAll)
            .map_err(Self::convert_error)?;
        Ok(())
    }

    async fn close(&self) -> StorageResult<()> {
        Ok(())
    }

    fn backend_type(&self) -> StorageBackendType {
        StorageBackendType::Fjall
    }

    #[inline]
    async fn stats(&self) -> StorageResult<StorageStats> {
        Ok(self.stats.to_storage_stats())
    }

    async fn compact(&self) -> StorageResult<()> {
        // No explicit compaction control available in Tx API
        Ok(())
    }
}

// Implement ApplicationDataStorage for FjallTxStorage
#[async_trait]
impl crate::ApplicationDataStorage for FjallTxStorage {
    type ReadTransaction<'a>
        = FjallTxTransaction<'a>
    where
        Self: 'a;
    type WriteTransaction<'a>
        = FjallTxTransaction<'a>
    where
        Self: 'a;

    fn begin_read_transaction(
        &self,
    ) -> Result<Self::ReadTransaction<'_>, StorageError> {
        self.begin_transaction()
    }

    fn begin_write_transaction(
        &self,
    ) -> Result<Self::WriteTransaction<'_>, StorageError> {
        self.begin_transaction()
    }

    async fn scan_range_paginated(
        &self,
        start: &[u8],
        end: &[u8],
        limit: Option<usize>,
    ) -> Result<
        (Vec<(StorageValue, StorageValue)>, Option<StorageValue>),
        StorageError,
    > {
        let range = start.to_vec()..=end.to_vec();
        let mut results = self.scan_range(range).await?;

        let next_key = if let Some(limit) = limit {
            if results.len() > limit {
                let next_item = results.split_off(limit);
                next_item.first().map(|(k, _)| k.clone())
            } else {
                None
            }
        } else {
            None
        };

        Ok((results, next_key))
    }

    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, StorageError> {
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            let value = self.get(key).await?;
            results.push(value);
        }

        Ok(results)
    }

    async fn compare_and_swap(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, StorageError> {
        let current = self.get(key).await?;

        let matches = match (&current, expected) {
            (None, None) => true,
            (Some(current_val), Some(expected_bytes)) => {
                current_val.as_slice() == expected_bytes
            }
            _ => false,
        };

        if matches {
            match new_value {
                Some(value) => {
                    self.put(key, value).await?;
                }
                None => {
                    self.delete(key).await?;
                }
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn increment(
        &self,
        key: &[u8],
        delta: i64,
    ) -> Result<i64, StorageError> {
        let current = self.get(key).await?;

        let current_value = match current {
            Some(value) => {
                let bytes = value.as_slice();
                if bytes.len() == 8 {
                    i64::from_le_bytes(bytes.try_into().map_err(|_| {
                        StorageError::serialization("Invalid i64 format")
                    })?)
                } else {
                    return Err(StorageError::serialization(
                        "Key does not contain i64 value",
                    ));
                }
            }
            None => 0i64,
        };

        let new_value = current_value.wrapping_add(delta);
        let new_bytes = new_value.to_le_bytes().to_vec();

        self.put(key, StorageValue::from(new_bytes)).await?;
        Ok(new_value)
    }

    async fn put_with_ttl(
        &self,
        key: &[u8],
        value: StorageValue,
        _ttl: std::time::Duration,
    ) -> Result<bool, StorageError> {
        // TTL not supported natively; perform regular put
        self.put(key, value).await
    }
}
