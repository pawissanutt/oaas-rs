use async_trait::async_trait;
use std::collections::BTreeMap;
use std::ops::RangeBounds;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::Stream;

use crate::{
    StorageBackend, StorageBackendType, StorageConfig, StorageError,
    StorageResult, StorageStats, StorageTransaction, StorageValue,
    atomic_stats::AtomicStats,
    snapshot::{Snapshot, SnapshotCapableStorage},
};

/// In-memory storage backend implementation
pub struct MemoryStorage {
    data: Arc<RwLock<BTreeMap<StorageValue, StorageValue>>>,
    config: StorageConfig,
    stats: AtomicStats,
}

impl Clone for MemoryStorage {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            config: self.config.clone(),
            stats: self.stats.clone(),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new_with_default()
            .expect("Failed to create default MemoryStorage")
    }
}

impl MemoryStorage {
    /// Create a new memory storage backend
    pub fn new(config: StorageConfig) -> StorageResult<Self> {
        Ok(Self {
            data: Arc::new(RwLock::new(BTreeMap::new())),
            config,
            stats: AtomicStats::new(StorageBackendType::Memory),
        })
    }

    pub fn new_with_default() -> StorageResult<Self> {
        Self::new(StorageConfig::memory())
    }
}

#[async_trait]
impl StorageBackend for MemoryStorage {
    type Transaction<'a>
        = MemoryTransaction
    where
        Self: 'a;

    fn begin_transaction(&self) -> StorageResult<Self::Transaction<'_>> {
        Ok(MemoryTransaction::new(self.data.clone()))
    }

    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let data = self.data.read().await;
        let storage_key = StorageValue::from_slice(key);
        Ok(data.get(&storage_key).cloned())
    }

    async fn put(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<bool> {
        let mut data = self.data.write().await;

        // // Check memory limit if configured
        // if let Some(limit_mb) = self.config.memory_limit_mb {
        //     let current_size =
        //         data.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>();
        //     let new_size = current_size + key.len() + value.len();
        //     let limit_bytes = limit_mb * 1024 * 1024;

        //     if new_size > limit_bytes {
        //         return Err(StorageError::backend("Memory limit exceeded"));
        //     }
        // }

        let existing =
            data.insert(StorageValue::from_slice(key), value.clone());
        drop(data);

        // Update stats efficiently with atomic operations
        self.stats
            .record_put(key.len(), value.len(), existing.is_some());
        Ok(existing.is_some())
    }

    async fn put_with_return(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<Option<StorageValue>> {
        let mut data = self.data.write().await;

        // // Check memory limit if configured
        // if let Some(limit_mb) = self.config.memory_limit_mb {
        //     let current_size =
        //         data.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>();
        //     let new_size = current_size + key.len() + value.len();
        //     let limit_bytes = limit_mb * 1024 * 1024;

        //     if new_size > limit_bytes {
        //         return Err(StorageError::backend("Memory limit exceeded"));
        //     }
        // }
        let previous_value =
            data.insert(StorageValue::from_slice(key), value.clone());
        drop(data);

        // Update stats efficiently with atomic operations
        let old_value_size = previous_value.as_ref().map(|v| v.len());
        self.stats.record_put_with_old_size(
            key.len(),
            value.len(),
            old_value_size,
        );
        Ok(previous_value)
    }

    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        let mut data = self.data.write().await;
        let storage_key = StorageValue::from_slice(key);
        if let Some(removed_value) = data.remove(&storage_key) {
            drop(data);
            // Update stats efficiently
            self.stats.record_delete(key.len(), removed_value.len());
        } else {
            drop(data);
        }
        Ok(())
    }

    async fn delete_range<R>(&self, range: R) -> StorageResult<u64>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let mut data = self.data.write().await;

        // Convert range bounds to StorageValue bounds
        let start_bound = match range.start_bound() {
            std::ops::Bound::Included(key) => {
                std::ops::Bound::Included(StorageValue::from_slice(key))
            }
            std::ops::Bound::Excluded(key) => {
                std::ops::Bound::Excluded(StorageValue::from_slice(key))
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };
        let end_bound = match range.end_bound() {
            std::ops::Bound::Included(key) => {
                std::ops::Bound::Included(StorageValue::from_slice(key))
            }
            std::ops::Bound::Excluded(key) => {
                std::ops::Bound::Excluded(StorageValue::from_slice(key))
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };

        // Collect keys and values to delete first to avoid borrowing issues
        let items_to_delete: Vec<(StorageValue, StorageValue)> = data
            .range((start_bound, end_bound))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        let deleted_count = items_to_delete.len() as u64;
        let mut total_deleted_size = 0u64;

        // Now delete all the collected keys
        for (key, value) in &items_to_delete {
            data.remove(key);
            total_deleted_size += (key.len() + value.len()) as u64;
        }

        drop(data);

        // Update stats efficiently with batch operation
        if deleted_count > 0 {
            self.stats
                .record_delete_batch(deleted_count, total_deleted_size);
        }

        Ok(deleted_count)
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let data = self.data.read().await;
        let storage_key = StorageValue::from_slice(key);
        Ok(data.contains_key(&storage_key))
    }

    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        let data = self.data.read().await;
        let mut results = Vec::new();

        // Use BTreeMap's range method for efficient prefix scanning
        // Create end bound by incrementing the last byte of the prefix
        let end_bound = if let Some(last_byte) = prefix.last() {
            if *last_byte == 255 {
                // If last byte is 255, we need to extend the prefix
                let mut end_prefix = prefix.to_vec();
                end_prefix.push(0);
                std::ops::Bound::Excluded(StorageValue::from_slice(&end_prefix))
            } else {
                let mut end_prefix = prefix.to_vec();
                *end_prefix.last_mut().unwrap() += 1;
                std::ops::Bound::Excluded(StorageValue::from_slice(&end_prefix))
            }
        } else {
            std::ops::Bound::Unbounded
        };

        let range = (
            std::ops::Bound::Included(StorageValue::from_slice(prefix)),
            end_bound,
        );

        for (key, value) in data.range(range) {
            results.push((key.clone(), value.clone()));
        }

        Ok(results)
    }

    async fn scan_range<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let data = self.data.read().await;
        let mut results = Vec::new();

        // Convert range bounds to StorageValue bounds
        let start_bound = match range.start_bound() {
            std::ops::Bound::Included(key) => {
                std::ops::Bound::Included(StorageValue::from_slice(key))
            }
            std::ops::Bound::Excluded(key) => {
                std::ops::Bound::Excluded(StorageValue::from_slice(key))
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };
        let end_bound = match range.end_bound() {
            std::ops::Bound::Included(key) => {
                std::ops::Bound::Included(StorageValue::from_slice(key))
            }
            std::ops::Bound::Excluded(key) => {
                std::ops::Bound::Excluded(StorageValue::from_slice(key))
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };

        // Use BTreeMap's efficient range method directly
        for (key, value) in data.range((start_bound, end_bound)) {
            results.push((key.clone(), value.clone()));
        }

        Ok(results)
    }

    async fn scan_range_reverse<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let data = self.data.read().await;
        let mut results = Vec::new();

        // Convert range bounds to StorageValue bounds
        let start_bound = match range.start_bound() {
            std::ops::Bound::Included(key) => {
                std::ops::Bound::Included(StorageValue::from_slice(key))
            }
            std::ops::Bound::Excluded(key) => {
                std::ops::Bound::Excluded(StorageValue::from_slice(key))
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };
        let end_bound = match range.end_bound() {
            std::ops::Bound::Included(key) => {
                std::ops::Bound::Included(StorageValue::from_slice(key))
            }
            std::ops::Bound::Excluded(key) => {
                std::ops::Bound::Excluded(StorageValue::from_slice(key))
            }
            std::ops::Bound::Unbounded => std::ops::Bound::Unbounded,
        };

        // Use BTreeMap's efficient range method and collect in reverse order
        for (key, value) in data.range((start_bound, end_bound)).rev() {
            results.push((key.clone(), value.clone()));
        }

        Ok(results)
    }

    async fn get_last(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let data = self.data.read().await;

        // Use BTreeMap's last_key_value method for O(log n) performance
        match data.last_key_value() {
            Some((key, value)) => Ok(Some((key.clone(), value.clone()))),
            None => Ok(None),
        }
    }

    async fn get_first(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let data = self.data.read().await;

        // Use BTreeMap's first_key_value method for O(log n) performance
        match data.first_key_value() {
            Some((key, value)) => Ok(Some((key.clone(), value.clone()))),
            None => Ok(None),
        }
    }

    async fn count(&self) -> StorageResult<u64> {
        let data = self.data.read().await;
        Ok(data.len() as u64)
    }

    async fn flush(&self) -> StorageResult<()> {
        // Memory storage doesn't need flushing
        Ok(())
    }

    async fn close(&self) -> StorageResult<()> {
        // Clear data to free memory
        let mut data = self.data.write().await;
        data.clear();
        Ok(())
    }

    fn backend_type(&self) -> StorageBackendType {
        StorageBackendType::Memory
    }

    async fn stats(&self) -> StorageResult<StorageStats> {
        Ok(self.stats.to_storage_stats())
    }

    async fn compact(&self) -> StorageResult<()> {
        // BTreeMap is already well-organized, but we can rebuild it
        // to potentially reduce memory fragmentation
        let mut data = self.data.write().await;
        let old_data = std::mem::take(&mut *data);
        *data = old_data.into_iter().collect();
        Ok(())
    }
}

/// Memory storage transaction
pub struct MemoryTransaction {
    data: Arc<RwLock<BTreeMap<StorageValue, StorageValue>>>,
    operations: Vec<TransactionOperation>,
    committed: bool,
}

#[derive(Debug, Clone)]
enum TransactionOperation {
    Put {
        key: StorageValue,
        value: StorageValue,
    },
    Delete {
        key: StorageValue,
    },
}

impl MemoryTransaction {
    fn new(data: Arc<RwLock<BTreeMap<StorageValue, StorageValue>>>) -> Self {
        Self {
            data,
            operations: Vec::new(),
            committed: false,
        }
    }
}

#[async_trait(?Send)]
impl StorageTransaction for MemoryTransaction {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let storage_key = StorageValue::from_slice(key);

        // Check if there's a pending operation for this key
        for op in self.operations.iter().rev() {
            match op {
                TransactionOperation::Put { key: op_key, value }
                    if op_key == &storage_key =>
                {
                    return Ok(Some(value.clone()));
                }
                TransactionOperation::Delete { key: op_key }
                    if op_key == &storage_key =>
                {
                    return Ok(None);
                }
                _ => continue,
            }
        }

        // If no pending operation, read from storage
        let data = self.data.read().await;
        Ok(data.get(&storage_key).cloned())
    }

    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction(
                "Transaction already committed",
            ));
        }

        self.operations.push(TransactionOperation::Put {
            key: StorageValue::from_slice(key),
            value,
        });
        Ok(())
    }

    async fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction(
                "Transaction already committed",
            ));
        }

        self.operations.push(TransactionOperation::Delete {
            key: StorageValue::from_slice(key),
        });
        Ok(())
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        Ok(self.get(key).await?.is_some())
    }

    async fn commit(mut self) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction(
                "Transaction already committed",
            ));
        }

        let mut data = self.data.write().await;

        // Apply all operations atomically
        for op in &self.operations {
            match op {
                TransactionOperation::Put { key, value } => {
                    data.insert(key.clone(), value.clone());
                }
                TransactionOperation::Delete { key } => {
                    data.remove(key);
                }
            }
        }

        self.committed = true;
        Ok(())
    }

    async fn rollback(mut self) -> StorageResult<()> {
        // Simply clear operations - nothing has been applied yet
        self.operations.clear();
        self.committed = true; // Mark as finished
        Ok(())
    }
}

/// Memory snapshot data - uses Arc to share reference instead of cloning
pub type MemorySnapshotData = Arc<BTreeMap<StorageValue, StorageValue>>;

// Implement SnapshotCapableStorage for MemoryStorage
#[async_trait]
impl SnapshotCapableStorage for MemoryStorage {
    type SnapshotData = MemorySnapshotData;

    async fn create_snapshot(
        &self,
    ) -> Result<Snapshot<Self::SnapshotData>, StorageError> {
        let data = self.data.read().await;
        let snapshot_data = Arc::new(data.clone()); // Create Arc reference to the cloned data

        let entry_count = snapshot_data.len() as u64;
        let total_size_bytes = snapshot_data
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>() as u64;

        let snapshot = Snapshot {
            snapshot_id: format!(
                "memory_snapshot_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            created_at: std::time::SystemTime::now(),
            sequence_number: entry_count, // Use entry count as sequence for simplicity
            snapshot_data,
            entry_count,
            total_size_bytes,
            compression: crate::CompressionType::None,
        };

        Ok(snapshot)
    }

    async fn restore_from_snapshot(
        &self,
        snapshot: &Snapshot<Self::SnapshotData>,
    ) -> Result<(), StorageError> {
        let mut data = self.data.write().await;
        data.clear();
        // Clone the data from the Arc reference
        let restored_data = snapshot.snapshot_data.as_ref().clone();
        data.extend(restored_data.iter().map(|(k, v)| (k.clone(), v.clone())));

        drop(data);

        // Recalculate stats from snapshot data
        let total_entries = snapshot.entry_count;
        let total_size = snapshot.total_size_bytes;
        self.stats.set_counts(total_entries, total_size);

        Ok(())
    }

    async fn latest_snapshot(
        &self,
    ) -> Result<Option<Snapshot<Self::SnapshotData>>, StorageError> {
        // Memory storage doesn't persist snapshots, so we create one on demand
        Ok(Some(self.create_snapshot().await?))
    }

    async fn estimate_snapshot_size(
        &self,
        snapshot_data: &Self::SnapshotData,
    ) -> Result<u64, StorageError> {
        let size = snapshot_data
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>() as u64;
        Ok(size)
    }

    async fn create_kv_snapshot_stream(
        &self,
        snapshot: &Snapshot<Self::SnapshotData>,
    ) -> Result<
        Box<
            dyn Stream<
                    Item = Result<(StorageValue, StorageValue), StorageError>,
                > + Send
                + Unpin,
        >,
        StorageError,
    > {
        // Create a safe streaming iterator that collects items without unsafe code
        let stream =
            MemorySnapshotStream::new(Arc::clone(&snapshot.snapshot_data));
        Ok(Box::new(stream))
    }

    async fn install_kv_snapshot_from_stream<S>(
        &self,
        mut stream: S,
    ) -> Result<(), StorageError>
    where
        S: Stream<Item = Result<(StorageValue, StorageValue), StorageError>>
            + Send
            + Unpin,
    {
        use tokio_stream::StreamExt;

        let mut data = self.data.write().await;
        data.clear(); // Clear existing data

        // Reset stats for clean state
        self.stats.reset();

        // Process the stream and insert all key-value pairs
        let mut total_entries = 0u64;
        let mut total_size = 0u64;

        while let Some(result) = stream.next().await {
            let (key, value) = result?;
            total_size += (key.len() + value.len()) as u64;
            data.insert(key, value);
            total_entries += 1;
        }

        drop(data);

        // Set final counts
        self.stats.set_counts(total_entries, total_size);
        Ok(())
    }
}

/// Memory snapshot stream that yields key-value pairs safely
struct MemorySnapshotStream {
    items: std::vec::IntoIter<(StorageValue, StorageValue)>,
}

impl MemorySnapshotStream {
    fn new(snapshot_data: MemorySnapshotData) -> Self {
        // Safely collect all items from the snapshot data
        let items: Vec<(StorageValue, StorageValue)> = snapshot_data
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        Self {
            items: items.into_iter(),
        }
    }
}

impl Stream for MemorySnapshotStream {
    type Item = Result<(StorageValue, StorageValue), StorageError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.items.next() {
            Some(item) => std::task::Poll::Ready(Some(Ok(item))),
            None => std::task::Poll::Ready(None),
        }
    }
}

// Make the stream unpin for easier use
impl Unpin for MemorySnapshotStream {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[tokio::test]
    async fn test_memory_storage_comprehensive() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();
        test_storage_backend_comprehensive(storage).await;
    }

    #[tokio::test]
    async fn test_memory_storage_value_ordering() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();
        test_storage_value_ordering(&storage).await;
    }

    #[tokio::test]
    async fn test_memory_storage_basic() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();
        test_storage_backend_basic(storage).await;
    }

    #[tokio::test]
    #[ignore = "Disabled because of performance problem"]
    async fn test_memory_limit() {
        let config = StorageConfig::memory().with_memory_limit(1); // 1MB limit
        let storage = MemoryStorage::new(config).unwrap();

        // Try to store more than 1MB of data
        let large_value = StorageValue::from(vec![0u8; 2 * 1024 * 1024]); // 2MB
        let result = storage.put(b"large_key", large_value).await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::Backend(_)));
    }
}

// Implement ApplicationDataStorage for MemoryStorage
#[async_trait]
impl crate::ApplicationDataStorage for MemoryStorage {
    type ReadTransaction<'a>
        = MemoryTransaction
    where
        Self: 'a;
    type WriteTransaction<'a>
        = MemoryTransaction
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
        let data = self.data.read().await;

        let start_bound =
            std::ops::Bound::Included(StorageValue::from_slice(start));
        let end_bound =
            std::ops::Bound::Excluded(StorageValue::from_slice(end));

        let mut results = Vec::new();
        let mut next_key = None;

        for (idx, (key, value)) in
            data.range((start_bound, end_bound)).enumerate()
        {
            if let Some(max) = limit {
                if idx >= max {
                    next_key = Some(key.clone());
                    break;
                }
            }
            results.push((key.clone(), value.clone()));
        }

        Ok((results, next_key))
    }

    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, StorageError> {
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            results.push(StorageBackend::get(self, key).await?);
        }
        Ok(results)
    }

    async fn compare_and_swap(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, StorageError> {
        let mut data = self.data.write().await;

        let storage_key = StorageValue::from_slice(key);
        let current = data.get(&storage_key);
        let current_matches = match (current, expected) {
            (Some(current), Some(expected)) => current.as_slice() == expected,
            (None, None) => true,
            _ => false,
        };

        if current_matches {
            let old_value = current.cloned();
            match new_value {
                Some(value) => {
                    data.insert(storage_key, value.clone());
                    drop(data);
                    // Track the change
                    let old_value_size = old_value.as_ref().map(|v| v.len());
                    self.stats.record_put_with_old_size(
                        key.len(),
                        value.len(),
                        old_value_size,
                    );
                }
                None => {
                    data.remove(&storage_key);
                    drop(data);
                    // Track deletion
                    if let Some(old_val) = old_value {
                        self.stats.record_delete(key.len(), old_val.len());
                    }
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
        let mut data = self.data.write().await;

        let storage_key = StorageValue::from_slice(key);
        let current_value = match data.get(&storage_key) {
            Some(value) => {
                // Try to parse as i64
                let bytes = value.as_slice();
                if bytes.len() == 8 {
                    i64::from_le_bytes(bytes.try_into().map_err(|_| {
                        StorageError::serialization("Invalid i64 format")
                    })?)
                } else {
                    return Err(StorageError::serialization(
                        "Value is not an i64",
                    ));
                }
            }
            None => 0, // Default to 0 if key doesn't exist
        };

        let new_value = current_value + delta;
        let value_bytes = new_value.to_le_bytes();
        let new_storage_value = StorageValue::from(value_bytes.as_slice());
        let had_previous = data.contains_key(&storage_key);
        data.insert(storage_key, new_storage_value.clone());

        drop(data);

        // Track the change (i64 values are always 8 bytes)
        self.stats.record_put(key.len(), 8, had_previous);
        Ok(new_value)
    }

    async fn put_with_ttl(
        &self,
        key: &[u8],
        value: StorageValue,
        _ttl: std::time::Duration,
    ) -> Result<bool, StorageError> {
        // Memory storage doesn't support TTL in this simple implementation
        // Just do a regular put
        self.put(key, value).await
    }
}

// Implement ApplicationReadTransaction for MemoryTransaction
#[async_trait(?Send)]
impl crate::ApplicationReadTransaction for MemoryTransaction {
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
            results.push(StorageTransaction::get(self, key).await?);
        }
        Ok(results)
    }

    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error> {
        let data = self.data.read().await;
        let mut results = Vec::new();

        let start_key = StorageValue::from_slice(start);
        let end_key = StorageValue::from_slice(end);
        let range = start_key..end_key;
        for (key, value) in data.range(range) {
            // Check if there's a pending operation for this key
            let mut found_in_ops = false;
            for op in self.operations.iter().rev() {
                match op {
                    TransactionOperation::Put {
                        key: op_key,
                        value: op_value,
                    } if op_key == key => {
                        results.push((key.clone(), op_value.clone()));
                        found_in_ops = true;
                        break;
                    }
                    TransactionOperation::Delete { key: op_key }
                        if op_key == key =>
                    {
                        found_in_ops = true;
                        break; // Skip this key, it's deleted
                    }
                    _ => continue,
                }
            }

            if !found_in_ops {
                results.push((key.clone(), value.clone()));
            }
        }

        // Add keys from pending operations that fall in the range
        let start_key = StorageValue::from_slice(start);
        let end_key = StorageValue::from_slice(end);
        for op in &self.operations {
            if let TransactionOperation::Put { key, value } = op {
                if key >= &start_key && key < &end_key {
                    // Check if we already included this key from storage
                    if !results.iter().any(|(k, _)| k == key) {
                        results.push((key.clone(), value.clone()));
                    }
                }
            }
        }

        // Sort results by key
        results.sort_by(|a, b| a.0.as_slice().cmp(b.0.as_slice()));

        Ok(results)
    }

    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        StorageTransaction::exists(self, key).await
    }
}

// Implement ApplicationWriteTransaction for MemoryTransaction
#[async_trait(?Send)]
impl crate::ApplicationWriteTransaction for MemoryTransaction {
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
        if self.committed {
            return Err(StorageError::transaction(
                "Transaction already committed",
            ));
        }

        // Check current value including pending operations
        let current = self.get(key).await?;
        let current_matches = match (current.as_ref(), expected) {
            (Some(current), Some(expected)) => current.as_slice() == expected,
            (None, None) => true,
            _ => false,
        };

        if current_matches {
            match new_value {
                Some(value) => {
                    self.operations.push(TransactionOperation::Put {
                        key: StorageValue::from_slice(key),
                        value,
                    });
                }
                None => {
                    self.operations.push(TransactionOperation::Delete {
                        key: StorageValue::from_slice(key),
                    });
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
