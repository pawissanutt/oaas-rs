use async_trait::async_trait;
use fjall::{Config, Keyspace, PartitionHandle};
use std::ops::RangeBounds;
use std::path::PathBuf;
use std::sync::Arc;

use crate::{
    StorageBackend, StorageBackendType, StorageConfig, StorageError,
    StorageResult, StorageStats, StorageValue, atomic_stats::AtomicStats,
};

use super::transaction::FjallTransaction;

/// Fjall storage backend implementation
pub struct FjallStorage {
    keyspace: Arc<Keyspace>,
    partition: Arc<PartitionHandle>,
    config: StorageConfig,
    stats: AtomicStats,
    path: PathBuf,
}

impl Clone for FjallStorage {
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

impl FjallStorage {
    /// Create a new Fjall storage backend
    pub fn new(config: StorageConfig) -> StorageResult<Self> {
        let path = config
            .path
            .as_ref()
            .ok_or_else(|| {
                StorageError::configuration(
                    "Path is required for Fjall storage".to_string(),
                )
            })?
            .clone();

        // Create Fjall config
        let mut fjall_config = Config::new(&path);

        // Configure based on storage config
        if let Some(cache_size) = config.cache_size_mb {
            fjall_config =
                fjall_config.cache_size((cache_size * 1024 * 1024) as u64); // Convert MB to bytes
        }

        // Note: Fjall doesn't expose write buffer size configuration in current version
        // if let Some(write_buffer_size) = config.write_buffer_size {
        //     fjall_config = fjall_config.max_write_buffer_size(write_buffer_size as u32);
        // }        // Open keyspace
        let keyspace = Keyspace::open(fjall_config).map_err(|e| {
            StorageError::backend(format!(
                "Failed to open Fjall keyspace: {}",
                e
            ))
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

    /// Create a new Fjall storage with default config
    pub fn new_with_default() -> StorageResult<Self> {
        let config = StorageConfig::default();
        Self::new(config)
    }

    /// Get access to the partition for snapshot operations
    pub fn partition(&self) -> &Arc<PartitionHandle> {
        &self.partition
    }

    /// Get access to the keyspace for snapshot operations  
    pub fn keyspace(&self) -> &Arc<Keyspace> {
        &self.keyspace
    }

    /// Convert Fjall error to StorageError
    pub(crate) fn convert_error(err: fjall::Error) -> StorageError {
        match err {
            fjall::Error::Io(io_err) => StorageError::Io(io_err),
            _ => StorageError::backend(err.to_string()),
        }
    }

    /// Convert snapshot-related errors to StorageError
    pub(crate) fn convert_snapshot_error<E: std::fmt::Display>(
        err: E,
    ) -> StorageError {
        StorageError::backend(err.to_string())
    }
}

#[async_trait]
impl StorageBackend for FjallStorage {
    type Transaction<'a>
        = FjallTransaction
    where
        Self: 'a;

    fn begin_transaction(&self) -> StorageResult<Self::Transaction<'_>> {
        FjallTransaction::new(Arc::clone(&self.partition))
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
        // Get the value size before deletion for stats tracking
        let value_size = if let Some(value) =
            self.partition.get(key).map_err(Self::convert_error)?
        {
            value.len()
        } else {
            0 // Key doesn't exist, so no size to track
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
        let mut count = 0u64;

        // Convert range bounds to bytes
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

        // Collect keys and their sizes to delete (to avoid iterator invalidation)
        let keys_and_sizes: Vec<(Vec<u8>, usize)> = self
            .partition
            .range::<&[u8], _>((start_bound, end_bound))
            .map(|result| result.map(|(k, v)| (k.to_vec(), k.len() + v.len())))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Self::convert_error)?;

        let total_size: u64 =
            keys_and_sizes.iter().map(|(_, size)| *size as u64).sum();

        // Delete collected keys
        for (key, _) in &keys_and_sizes {
            self.partition.remove(key).map_err(Self::convert_error)?;
            count += 1;
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
        let results: Vec<(StorageValue, StorageValue)> = self
            .partition
            .prefix(prefix)
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
        // Convert range bounds to bytes
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

        let results: Vec<(StorageValue, StorageValue)> = self
            .partition
            .range::<&[u8], _>((start_bound, end_bound))
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
        // Convert range bounds to bytes
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

        let results: Vec<(StorageValue, StorageValue)> = self
            .partition
            .range::<&[u8], _>((start_bound, end_bound))
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
            .iter()
            .rev()
            .next()
            .transpose()
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
            .iter()
            .next()
            .transpose()
            .map_err(Self::convert_error)?
            .map(|(k, v)| {
                (StorageValue::from_slice(&k), StorageValue::from_slice(&v))
            });

        Ok(result)
    }

    async fn count(&self) -> StorageResult<u64> {
        // Fjall doesn't have exact count, so we approximate
        let count = self.partition.approximate_len() as u64;
        Ok(count)
    }

    async fn flush(&self) -> StorageResult<()> {
        // Fjall handles flushing automatically, but we can trigger a sync
        // by accessing the keyspace (which ensures data consistency)
        Ok(())
    }

    async fn close(&self) -> StorageResult<()> {
        // Fjall handles cleanup automatically when dropped
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
        // Trigger compaction if available in Fjall
        // Note: Fjall handles compaction automatically
        Ok(())
    }
}

// Implement ApplicationDataStorage for FjallStorage
#[async_trait]
impl crate::ApplicationDataStorage for FjallStorage {
    type ReadTransaction<'a>
        = FjallTransaction
    where
        Self: 'a;
    type WriteTransaction<'a>
        = FjallTransaction
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

        // Check if current value matches expected
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
        // Fjall doesn't support TTL natively, so we just do a regular put
        // In a real implementation, you might want to use a separate thread
        // or external mechanism to handle TTL
        self.put(key, value).await
    }
}
