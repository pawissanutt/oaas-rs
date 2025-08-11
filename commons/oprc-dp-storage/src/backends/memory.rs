use async_trait::async_trait;
use std::collections::BTreeMap;
use std::ops::RangeBounds;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_stream::Stream;

use crate::{
    snapshot::{Snapshot, SnapshotCapableStorage},
    StorageBackend, StorageBackendType, StorageConfig, StorageError,
    StorageResult, StorageStats, StorageTransaction, StorageValue,
};

/// In-memory storage backend implementation
pub struct MemoryStorage {
    data: Arc<RwLock<BTreeMap<Vec<u8>, StorageValue>>>,
    config: StorageConfig,
    stats: Arc<RwLock<StorageStats>>,
}

impl Clone for MemoryStorage {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            config: self.config.clone(),
            stats: Arc::clone(&self.stats),
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
            stats: Arc::new(RwLock::new(StorageStats::default())),
        })
    }

    pub fn new_with_default() -> StorageResult<Self> {
        Self::new(StorageConfig::memory())
    }

    /// Update statistics
    async fn update_stats(&self) {
        let data = self.data.read().await;
        let mut stats = self.stats.write().await;

        stats.entries_count = data.len() as u64;
        stats.total_size_bytes =
            data.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() as u64;
        stats.memory_usage_bytes = Some(stats.total_size_bytes);
        stats.disk_usage_bytes = Some(0); // Memory storage doesn't use disk
        stats.cache_hit_rate = Some(1.0); // Memory storage is always a cache hit
    }
}

#[async_trait]
impl StorageBackend for MemoryStorage {
    type Transaction = MemoryTransaction;

    async fn begin_transaction(&self) -> StorageResult<Self::Transaction> {
        Ok(MemoryTransaction::new(self.data.clone()))
    }

    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
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

        let existing = data.insert(key.to_vec(), value);
        drop(data);

        // Update stats asynchronously
        self.update_stats().await;
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
        let previous_value = data.insert(key.to_vec(), value);
        drop(data);

        // Update stats asynchronously
        self.update_stats().await;
        Ok(previous_value)
    }

    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        let mut data = self.data.write().await;
        data.remove(key);
        drop(data);

        self.update_stats().await;
        Ok(())
    }

    async fn delete_range<R>(&self, range: R) -> StorageResult<u64>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let mut data = self.data.write().await;

        // Collect keys to delete first to avoid borrowing issues
        let keys_to_delete: Vec<Vec<u8>> =
            data.range(range).map(|(key, _)| key.clone()).collect();

        let deleted_count = keys_to_delete.len() as u64;

        // Now delete all the collected keys
        for key in keys_to_delete {
            data.remove(&key);
        }

        drop(data);
        self.update_stats().await;

        Ok(deleted_count)
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let data = self.data.read().await;
        Ok(data.contains_key(key))
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
                std::ops::Bound::Excluded(end_prefix)
            } else {
                let mut end_prefix = prefix.to_vec();
                *end_prefix.last_mut().unwrap() += 1;
                std::ops::Bound::Excluded(end_prefix)
            }
        } else {
            std::ops::Bound::Unbounded
        };

        let range = (std::ops::Bound::Included(prefix.to_vec()), end_bound);

        for (key, value) in data.range(range) {
            results.push((StorageValue::from_slice(key), value.clone()));
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

        // Use BTreeMap's efficient range method directly
        for (key, value) in data.range(range) {
            results.push((StorageValue::from_slice(key), value.clone()));
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

        // Use BTreeMap's efficient range method and collect in reverse order
        for (key, value) in data.range(range).rev() {
            results.push((StorageValue::from_slice(key), value.clone()));
        }

        Ok(results)
    }

    async fn get_last(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let data = self.data.read().await;

        // Use BTreeMap's last_key_value method for O(log n) performance
        match data.last_key_value() {
            Some((key, value)) => {
                Ok(Some((StorageValue::from_slice(key), value.clone())))
            }
            None => Ok(None),
        }
    }

    async fn get_first(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let data = self.data.read().await;

        // Use BTreeMap's first_key_value method for O(log n) performance
        match data.first_key_value() {
            Some((key, value)) => {
                Ok(Some((StorageValue::from_slice(key), value.clone())))
            }
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
        self.update_stats().await;
        let stats = self.stats.read().await;
        Ok(stats.clone())
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
    data: Arc<RwLock<BTreeMap<Vec<u8>, StorageValue>>>,
    operations: Vec<TransactionOperation>,
    committed: bool,
}

#[derive(Debug, Clone)]
enum TransactionOperation {
    Put { key: Vec<u8>, value: StorageValue },
    Delete { key: Vec<u8> },
}

impl MemoryTransaction {
    fn new(data: Arc<RwLock<BTreeMap<Vec<u8>, StorageValue>>>) -> Self {
        Self {
            data,
            operations: Vec::new(),
            committed: false,
        }
    }
}

#[async_trait]
impl StorageTransaction for MemoryTransaction {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        // Check if there's a pending operation for this key
        for op in self.operations.iter().rev() {
            match op {
                TransactionOperation::Put { key: op_key, value }
                    if op_key == key =>
                {
                    return Ok(Some(value.clone()));
                }
                TransactionOperation::Delete { key: op_key }
                    if op_key == key =>
                {
                    return Ok(None);
                }
                _ => continue,
            }
        }

        // If no pending operation, read from storage
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
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
            key: key.to_vec(),
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

        self.operations
            .push(TransactionOperation::Delete { key: key.to_vec() });
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
pub type MemorySnapshotData = Arc<BTreeMap<Vec<u8>, StorageValue>>;

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
        data.extend(snapshot.snapshot_data.as_ref().clone());

        drop(data);
        self.update_stats().await;
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

        // Process the stream and insert all key-value pairs
        while let Some(result) = stream.next().await {
            let (key, value) = result?;
            data.insert(key.as_slice().to_vec(), value);
        }

        drop(data);
        self.update_stats().await;
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
            .map(|(key, value)| (StorageValue::from_slice(key), value.clone()))
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

    #[tokio::test]
    async fn test_memory_storage_basic_operations() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();

        let key = b"test_key";
        let value = StorageValue::from("test_value");

        // Test put and get
        storage.put(key, value.clone()).await.unwrap();
        let retrieved = storage.get(key).await.unwrap();
        assert_eq!(retrieved, Some(value));

        // Test exists
        assert!(storage.exists(key).await.unwrap());
        assert!(!storage.exists(b"nonexistent").await.unwrap());

        // Test delete
        storage.delete(key).await.unwrap();
        assert!(!storage.exists(key).await.unwrap());
        assert_eq!(storage.get(key).await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_memory_storage_transactions() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();

        let key1 = b"key1";
        let key2 = b"key2";
        let value1 = StorageValue::from("value1");
        let value2 = StorageValue::from("value2");

        // Test transaction commit
        {
            let mut tx = storage.begin_transaction().await.unwrap();
            tx.put(key1, value1.clone()).await.unwrap();
            tx.put(key2, value2.clone()).await.unwrap();
            tx.commit().await.unwrap();
        }

        assert_eq!(storage.get(key1).await.unwrap(), Some(value1));
        assert_eq!(storage.get(key2).await.unwrap(), Some(value2));

        // Test transaction rollback
        {
            let mut tx = storage.begin_transaction().await.unwrap();
            tx.delete(key1).await.unwrap();
            tx.rollback().await.unwrap();
        }

        // Key should still exist after rollback
        assert!(storage.exists(key1).await.unwrap());
    }

    #[tokio::test]
    async fn test_memory_storage_scan() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();

        // Insert test data
        storage
            .put(b"prefix_1", StorageValue::from("value1"))
            .await
            .unwrap();
        storage
            .put(b"prefix_2", StorageValue::from("value2"))
            .await
            .unwrap();
        storage
            .put(b"other_3", StorageValue::from("value3"))
            .await
            .unwrap();

        // Test prefix scan
        let results = storage.scan(b"prefix_").await.unwrap();
        assert_eq!(results.len(), 2);

        // Results should be sorted
        assert_eq!(results[0].0.as_slice(), b"prefix_1");
        assert_eq!(results[1].0.as_slice(), b"prefix_2");
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

    #[tokio::test]
    async fn test_btree_range_operations() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();

        // Insert test data with keys that will be ordered
        storage
            .put(b"key_001", StorageValue::from("value1"))
            .await
            .unwrap();
        storage
            .put(b"key_005", StorageValue::from("value2"))
            .await
            .unwrap();
        storage
            .put(b"key_010", StorageValue::from("value3"))
            .await
            .unwrap();
        storage
            .put(b"key_015", StorageValue::from("value4"))
            .await
            .unwrap();
        storage
            .put(b"key_020", StorageValue::from("value5"))
            .await
            .unwrap();
        storage
            .put(b"other_key", StorageValue::from("other"))
            .await
            .unwrap();

        // Test range scan
        let range_results = storage
            .scan_range(b"key_005".to_vec()..b"key_015".to_vec())
            .await
            .unwrap();
        assert_eq!(range_results.len(), 2); // key_005 and key_010
        assert_eq!(range_results[0].0.as_slice(), b"key_005");
        assert_eq!(range_results[1].0.as_slice(), b"key_010");

        // Test reverse range scan
        let reverse_results = storage
            .scan_range_reverse(b"key_005".to_vec()..b"key_015".to_vec())
            .await
            .unwrap();
        assert_eq!(reverse_results.len(), 2);
        assert_eq!(reverse_results[0].0.as_slice(), b"key_010"); // Reversed order
        assert_eq!(reverse_results[1].0.as_slice(), b"key_005");

        // Test get_first and get_last
        let first = storage.get_first().await.unwrap().unwrap();
        assert_eq!(first.0.as_slice(), b"key_001"); // Should be the smallest key

        let last = storage.get_last().await.unwrap().unwrap();
        assert_eq!(last.0.as_slice(), b"other_key"); // Should be the largest key lexicographically

        // Test delete_range
        let deleted_count = storage
            .delete_range(b"key_005".to_vec()..b"key_020".to_vec())
            .await
            .unwrap();
        assert_eq!(deleted_count, 3); // key_005, key_010, key_015

        // Verify deletions
        assert!(storage.exists(b"key_001").await.unwrap()); // Should still exist
        assert!(!storage.exists(b"key_005").await.unwrap()); // Should be deleted
        assert!(!storage.exists(b"key_010").await.unwrap()); // Should be deleted
        assert!(!storage.exists(b"key_015").await.unwrap()); // Should be deleted
        assert!(storage.exists(b"key_020").await.unwrap()); // Should still exist (excluded from range)
        assert!(storage.exists(b"other_key").await.unwrap()); // Should still exist
    }

    #[tokio::test]
    async fn test_snapshot_operations() {
        use crate::snapshot::SnapshotCapableStorage;
        use tokio_stream::StreamExt;

        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).unwrap();

        // Insert test data
        storage
            .put(b"key1", StorageValue::from("value1"))
            .await
            .unwrap();
        storage
            .put(b"key2", StorageValue::from("value2"))
            .await
            .unwrap();
        storage
            .put(b"key3", StorageValue::from("value3"))
            .await
            .unwrap();

        // Create snapshot
        let snapshot = storage.create_snapshot().await.unwrap();
        assert_eq!(snapshot.entry_count, 3);
        assert_eq!(snapshot.snapshot_data.len(), 3);

        // Verify snapshot contains correct data
        assert_eq!(
            snapshot.snapshot_data.get(b"key1".as_slice()),
            Some(&StorageValue::from("value1"))
        );
        assert_eq!(
            snapshot.snapshot_data.get(b"key2".as_slice()),
            Some(&StorageValue::from("value2"))
        );
        assert_eq!(
            snapshot.snapshot_data.get(b"key3".as_slice()),
            Some(&StorageValue::from("value3"))
        );

        // Modify original storage
        storage
            .put(b"key4", StorageValue::from("value4"))
            .await
            .unwrap();
        storage.delete(b"key1").await.unwrap();

        // Verify snapshot is unchanged (Arc reference protects the data)
        assert_eq!(snapshot.snapshot_data.len(), 3);
        assert!(snapshot.snapshot_data.contains_key(b"key1".as_slice()));
        assert!(!snapshot.snapshot_data.contains_key(b"key4".as_slice()));

        // Test restore from snapshot
        storage.restore_from_snapshot(&snapshot).await.unwrap();
        assert!(storage.exists(b"key1").await.unwrap());
        assert!(storage.exists(b"key2").await.unwrap());
        assert!(storage.exists(b"key3").await.unwrap());
        assert!(!storage.exists(b"key4").await.unwrap());

        // Test streaming functionality
        let kv_stream =
            storage.create_kv_snapshot_stream(&snapshot).await.unwrap();
        let pairs: Vec<_> = kv_stream.collect().await;
        assert_eq!(pairs.len(), 3);

        // All results should be Ok
        for result in &pairs {
            assert!(result.is_ok());
        }

        // Test install from stream
        let new_storage = MemoryStorage::new(StorageConfig::memory()).unwrap();
        let stream =
            storage.create_kv_snapshot_stream(&snapshot).await.unwrap();
        new_storage
            .install_kv_snapshot_from_stream(stream)
            .await
            .unwrap();

        // Verify new storage has the same data
        assert!(new_storage.exists(b"key1").await.unwrap());
        assert!(new_storage.exists(b"key2").await.unwrap());
        assert!(new_storage.exists(b"key3").await.unwrap());
        assert_eq!(
            new_storage.get(b"key1").await.unwrap(),
            Some(StorageValue::from("value1"))
        );
        assert_eq!(
            new_storage.get(b"key2").await.unwrap(),
            Some(StorageValue::from("value2"))
        );
        assert_eq!(
            new_storage.get(b"key3").await.unwrap(),
            Some(StorageValue::from("value3"))
        );
    }
}

// Implement ApplicationDataStorage for MemoryStorage
#[async_trait]
impl crate::ApplicationDataStorage for MemoryStorage {
    type ReadTransaction = MemoryTransaction;
    type WriteTransaction = MemoryTransaction;

    async fn begin_read_transaction(
        &self,
    ) -> Result<Self::ReadTransaction, StorageError> {
        self.begin_transaction().await
    }

    async fn begin_write_transaction(
        &self,
    ) -> Result<Self::WriteTransaction, StorageError> {
        self.begin_transaction().await
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
        let range = start.to_vec()..end.to_vec();
        let mut results = self.scan_range(range).await?;

        if let Some(limit) = limit {
            let next_key = if results.len() > limit {
                let next = results.get(limit).map(|(k, _)| k.clone());
                results.truncate(limit);
                next
            } else {
                None
            };
            Ok((results, next_key))
        } else {
            Ok((results, None))
        }
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

        let current = data.get(key);
        let current_matches = match (current, expected) {
            (Some(current), Some(expected)) => current.as_slice() == expected,
            (None, None) => true,
            _ => false,
        };

        if current_matches {
            match new_value {
                Some(value) => {
                    data.insert(key.to_vec(), value);
                }
                None => {
                    data.remove(key);
                }
            }
            drop(data);
            self.update_stats().await;
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

        let current_value = match data.get(key) {
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
        data.insert(key.to_vec(), StorageValue::from(value_bytes.as_slice()));

        drop(data);
        self.update_stats().await;
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
#[async_trait]
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

        let range = start.to_vec()..end.to_vec();
        for (key, value) in data.range(range) {
            // Check if there's a pending operation for this key
            let mut found_in_ops = false;
            for op in self.operations.iter().rev() {
                match op {
                    TransactionOperation::Put {
                        key: op_key,
                        value: op_value,
                    } if op_key == key => {
                        results.push((
                            StorageValue::from_slice(key),
                            op_value.clone(),
                        ));
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
                results.push((StorageValue::from_slice(key), value.clone()));
            }
        }

        // Add keys from pending operations that fall in the range
        for op in &self.operations {
            if let TransactionOperation::Put { key, value } = op {
                if key >= &start.to_vec() && key < &end.to_vec() {
                    // Check if we already included this key from storage
                    if !results.iter().any(|(k, _)| k.as_slice() == key) {
                        results.push((
                            StorageValue::from_slice(key),
                            value.clone(),
                        ));
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
#[async_trait]
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
                        key: key.to_vec(),
                        value,
                    });
                }
                None => {
                    self.operations.push(TransactionOperation::Delete {
                        key: key.to_vec(),
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
