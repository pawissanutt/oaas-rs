use async_trait::async_trait;
use crossbeam_skiplist::SkipMap;
use std::ops::RangeBounds;
use std::sync::Arc;
use tokio_stream::Stream;

use crate::{
    atomic_stats::AtomicStats,
    snapshot::{Snapshot, SnapshotCapableStorage},
    StorageBackend, StorageBackendType, StorageConfig, StorageError,
    StorageResult, StorageStats, StorageTransaction, StorageValue,
};

/// SkipList-based in-memory storage backend implementation using crossbeam-skiplist
pub struct SkipListStorage {
    data: Arc<SkipMap<StorageValue, StorageValue>>,
    config: StorageConfig,
    stats: AtomicStats,
}

impl Clone for SkipListStorage {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            config: self.config.clone(),
            stats: self.stats.clone(),
        }
    }
}

impl Default for SkipListStorage {
    fn default() -> Self {
        Self::new_with_default()
            .expect("Failed to create default SkipListStorage")
    }
}

impl SkipListStorage {
    /// Create a new SkipList storage backend
    pub fn new(config: StorageConfig) -> StorageResult<Self> {
        Ok(Self {
            data: Arc::new(SkipMap::new()),
            config,
            stats: AtomicStats::new(StorageBackendType::Memory),
        })
    }

    pub fn new_with_default() -> StorageResult<Self> {
        Self::new(StorageConfig::memory())
    }
}

#[async_trait]
impl StorageBackend for SkipListStorage {
    type Transaction<'a> = SkipListTransaction where Self: 'a;

    fn begin_transaction(&self) -> StorageResult<Self::Transaction<'_>> {
        Ok(SkipListTransaction::new(self.data.clone()))
    }

    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let storage_key = StorageValue::from_slice(key);
        Ok(self
            .data
            .get(&storage_key)
            .map(|entry| entry.value().clone()))
    }

    async fn put(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<bool> {
        let storage_key = StorageValue::from_slice(key);
        let existing = self.data.get(&storage_key).is_some();
        self.data.insert(storage_key, value.clone());

        self.stats.record_put(key.len(), value.len(), existing);
        Ok(existing)
    }

    async fn put_with_return(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<Option<StorageValue>> {
        let storage_key = StorageValue::from_slice(key);
        let previous_value = self
            .data
            .get(&storage_key)
            .map(|entry| entry.value().clone());

        self.data.insert(storage_key, value.clone());

        let old_value_size = previous_value.as_ref().map(|v| v.len());
        self.stats.record_put_with_old_size(
            key.len(),
            value.len(),
            old_value_size,
        );

        Ok(previous_value)
    }

    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        let storage_key = StorageValue::from_slice(key);

        // Get the size before removing to update stats
        if let Some(entry) = self.data.get(&storage_key) {
            let key_size = entry.key().len();
            let value_size = entry.value().len();
            drop(entry); // Release the reference before removal

            self.data.remove(&storage_key);
            self.stats.record_delete(key_size, value_size);
        }

        Ok(())
    }

    async fn delete_range<R>(&self, range: R) -> StorageResult<u64>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
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

        // Collect keys and their sizes to delete first
        let keys_to_delete: Vec<(StorageValue, usize, usize)> = self
            .data
            .range((start_bound, end_bound))
            .map(|entry| {
                let key = entry.key().clone();
                let key_size = entry.key().len();
                let value_size = entry.value().len();
                (key, key_size, value_size)
            })
            .collect();

        let deleted_count = keys_to_delete.len() as u64;

        // Delete all collected keys and update stats in batch
        let mut total_entries_removed = 0u64;
        let mut total_size_removed = 0u64;

        for (key, key_size, value_size) in keys_to_delete {
            self.data.remove(&key);
            total_entries_removed += 1;
            total_size_removed += (key_size + value_size) as u64;
        }

        // Update stats in batch for better performance
        if total_entries_removed > 0 {
            self.stats
                .record_delete_batch(total_entries_removed, total_size_removed);
        }

        Ok(deleted_count)
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let storage_key = StorageValue::from_slice(key);
        Ok(self.data.contains_key(&storage_key))
    }

    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        let mut results = Vec::new();
        let prefix_storage = StorageValue::from_slice(prefix);

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

        let range = (std::ops::Bound::Included(prefix_storage), end_bound);

        for entry in self.data.range(range) {
            results.push((entry.key().clone(), entry.value().clone()));
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

        for entry in self.data.range((start_bound, end_bound)) {
            results.push((entry.key().clone(), entry.value().clone()));
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

        // Collect in reverse order
        let mut entries: Vec<_> =
            self.data.range((start_bound, end_bound)).collect();
        entries.reverse();

        for entry in entries {
            results.push((entry.key().clone(), entry.value().clone()));
        }

        Ok(results)
    }

    async fn get_last(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        match self.data.back() {
            Some(entry) => {
                Ok(Some((entry.key().clone(), entry.value().clone())))
            }
            None => Ok(None),
        }
    }

    async fn get_first(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        match self.data.front() {
            Some(entry) => {
                Ok(Some((entry.key().clone(), entry.value().clone())))
            }
            None => Ok(None),
        }
    }

    async fn count(&self) -> StorageResult<u64> {
        Ok(self.data.len() as u64)
    }

    async fn flush(&self) -> StorageResult<()> {
        // SkipList storage doesn't need flushing
        Ok(())
    }

    async fn compact(&self) -> StorageResult<()> {
        // SkipList storage doesn't need compaction
        Ok(())
    }

    async fn close(&self) -> StorageResult<()> {
        // Clear data to free memory
        self.data.clear();
        Ok(())
    }

    fn backend_type(&self) -> StorageBackendType {
        StorageBackendType::Memory
    }

    async fn stats(&self) -> StorageResult<StorageStats> {
        Ok(self.stats.to_storage_stats())
    }
}

/// Transaction implementation for SkipListStorage
pub struct SkipListTransaction {
    data: Arc<SkipMap<StorageValue, StorageValue>>,
}

impl SkipListTransaction {
    pub fn new(data: Arc<SkipMap<StorageValue, StorageValue>>) -> Self {
        Self { data }
    }
}

#[async_trait(?Send)]
impl StorageTransaction for SkipListTransaction {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let storage_key = StorageValue::from_slice(key);
        Ok(self
            .data
            .get(&storage_key)
            .map(|entry| entry.value().clone()))
    }

    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()> {
        let storage_key = StorageValue::from_slice(key);
        self.data.insert(storage_key, value);
        Ok(())
    }

    async fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        let storage_key = StorageValue::from_slice(key);
        self.data.remove(&storage_key);
        Ok(())
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let storage_key = StorageValue::from_slice(key);
        Ok(self.data.contains_key(&storage_key))
    }

    async fn commit(self) -> StorageResult<()> {
        // SkipMap operations are already atomic, so commit is a no-op
        Ok(())
    }

    async fn rollback(self) -> StorageResult<()> {
        // For this simple implementation, rollback is not supported
        // In a real implementation, you'd need to track changes
        Err(StorageError::backend(
            "Rollback not supported in SkipList backend",
        ))
    }
}

/// SkipList snapshot data - uses Arc to share reference instead of cloning
pub type SkipListSnapshotData = Arc<Vec<(StorageValue, StorageValue)>>;

// Implement SnapshotCapableStorage for SkipListStorage
#[async_trait]
impl SnapshotCapableStorage for SkipListStorage {
    type SnapshotData = SkipListSnapshotData;

    async fn create_snapshot(
        &self,
    ) -> Result<Snapshot<Self::SnapshotData>, StorageError> {
        // Collect all data from SkipMap
        let snapshot_data: Vec<(StorageValue, StorageValue)> = self
            .data
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();

        let arc_data = Arc::new(snapshot_data);
        let entry_count = arc_data.len() as u64;
        let total_size_bytes = arc_data
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>() as u64;

        let snapshot = Snapshot {
            snapshot_id: format!(
                "skiplist_snapshot_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ),
            created_at: std::time::SystemTime::now(),
            sequence_number: entry_count, // Use entry count as sequence for simplicity
            snapshot_data: arc_data,
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
        self.data.clear();

        // Restore all data from snapshot and calculate stats
        let mut total_entries = 0u64;
        let mut total_size = 0u64;

        for (key, value) in snapshot.snapshot_data.iter() {
            self.data.insert(key.clone(), value.clone());
            total_entries += 1;
            total_size += (key.len() + value.len()) as u64;
        }

        // Update atomic counters to match restored data
        self.stats.set_counts(total_entries, total_size);

        Ok(())
    }

    async fn latest_snapshot(
        &self,
    ) -> Result<Option<Snapshot<Self::SnapshotData>>, StorageError> {
        // SkipList storage doesn't persist snapshots, so we create one on demand
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
        // Create a safe streaming iterator
        let stream =
            SkipListSnapshotStream::new(Arc::clone(&snapshot.snapshot_data));
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

        self.data.clear(); // Clear existing data

        // Reset atomic counters
        self.stats.reset();

        // Process the stream and insert all key-value pairs
        while let Some(result) = stream.next().await {
            let (key, value) = result?;
            let key_size = key.len();
            let value_size = value.len();

            self.data.insert(key.clone(), value.clone());

            // Update counters atomically for each insert
            self.stats.record_put(key_size, value_size, false);
        }

        Ok(())
    }
}

/// SkipList snapshot stream that yields key-value pairs safely
struct SkipListSnapshotStream {
    items: std::vec::IntoIter<(StorageValue, StorageValue)>,
}

impl SkipListSnapshotStream {
    fn new(snapshot_data: SkipListSnapshotData) -> Self {
        // Clone the data from the Arc and create an iterator
        let items = snapshot_data.as_ref().clone();

        Self {
            items: items.into_iter(),
        }
    }
}

impl Stream for SkipListSnapshotStream {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::*;

    #[tokio::test]
    async fn test_skiplist_storage_comprehensive() {
        let config = StorageConfig::memory();
        let storage = SkipListStorage::new(config).unwrap();
        test_storage_backend_comprehensive(storage).await;
    }

    #[tokio::test]
    async fn test_skiplist_storage_value_ordering() {
        let config = StorageConfig::memory();
        let storage = SkipListStorage::new(config).unwrap();
        test_storage_value_ordering(&storage).await;
    }

    #[tokio::test]
    async fn test_skiplist_storage_basic() {
        let config = StorageConfig::memory();
        let storage = SkipListStorage::new(config).unwrap();
        test_storage_backend_basic(storage).await;
    }

    #[tokio::test]
    async fn test_skiplist_concurrent_access() {
        // Test concurrent access capabilities specific to SkipList
        let config = StorageConfig::memory();
        let storage = Arc::new(SkipListStorage::new(config).unwrap());

        // Spawn multiple tasks that write and read concurrently
        let mut handles = vec![];

        for i in 0..10 {
            let storage_clone = storage.clone();
            let handle = tokio::spawn(async move {
                let key = format!("concurrent_key_{}", i);
                let value =
                    StorageValue::from(format!("concurrent_value_{}", i));

                // Write
                storage_clone
                    .put(key.as_bytes(), value.clone())
                    .await
                    .unwrap();

                // Read back
                let retrieved =
                    storage_clone.get(key.as_bytes()).await.unwrap();
                assert_eq!(retrieved, Some(value));
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all data is present
        for i in 0..10 {
            let key = format!("concurrent_key_{}", i);
            assert!(storage.exists(key.as_bytes()).await.unwrap());
        }
    }
}
