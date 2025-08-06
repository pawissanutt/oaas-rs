use crate::{StorageResult, StorageValue};
use async_trait::async_trait;
use std::ops::{Range, RangeBounds};

/// Core storage backend trait that abstracts different storage engines
#[async_trait]
pub trait StorageBackend: Send + Sync {
    type Transaction: StorageTransaction;

    /// Begin a new transaction
    async fn begin_transaction(&self) -> StorageResult<Self::Transaction>;

    /// Get a value by key
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>>;

    /// Put a key-value pair
    async fn put(&self, key: &[u8], value: StorageValue) -> StorageResult<()>;

    /// Delete a key
    async fn delete(&self, key: &[u8]) -> StorageResult<()>;

    /// Delete all keys in a range
    async fn delete_range<R>(&self, range: R) -> StorageResult<u64>
    where
        R: RangeBounds<Vec<u8>> + Send;

    /// Check if a key exists
    async fn exists(&self, key: &[u8]) -> StorageResult<bool>;

    /// Scan keys with a prefix
    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Scan keys in a range
    async fn scan_range<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send;

    /// Scan keys in a range in reverse order (largest to smallest)
    async fn scan_range_reverse<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send;

    /// Get the last key-value pair (largest key)
    async fn get_last(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>>;

    /// Get the first key-value pair (smallest key)
    async fn get_first(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>>;

    /// Get the number of entries
    async fn count(&self) -> StorageResult<u64>;

    /// Flush any pending writes
    async fn flush(&self) -> StorageResult<()>;

    /// Close the storage backend
    async fn close(&self) -> StorageResult<()>;

    /// Get backend type information
    fn backend_type(&self) -> crate::StorageBackendType;

    /// Get backend statistics
    async fn stats(&self) -> StorageResult<crate::StorageStats>;

    /// Compact/optimize the storage (if supported)
    async fn compact(&self) -> StorageResult<()>;
}

/// Transaction trait for atomic operations
#[async_trait]
pub trait StorageTransaction: Send + Sync + Sized {
    /// Get a value by key within the transaction
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>>;

    /// Put a key-value pair within the transaction
    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()>;

    /// Delete a key within the transaction
    async fn delete(&mut self, key: &[u8]) -> StorageResult<()>;

    /// Check if a key exists within the transaction
    async fn exists(&self, key: &[u8]) -> StorageResult<bool>;

    /// Commit the transaction
    async fn commit(self) -> StorageResult<()>;

    /// Rollback the transaction
    async fn rollback(self) -> StorageResult<()>;

    /// Batch put operations
    async fn batch_put(
        &mut self,
        entries: Vec<(&[u8], StorageValue)>,
    ) -> StorageResult<()> {
        for (key, value) in entries {
            self.put(key, value).await?;
        }
        Ok(())
    }

    /// Batch delete operations
    async fn batch_delete(&mut self, keys: Vec<&[u8]>) -> StorageResult<()> {
        for key in keys {
            self.delete(key).await?;
        }
        Ok(())
    }
}

/// Batch write trait for efficient bulk operations
#[async_trait]
pub trait BatchWrite {
    /// Execute a batch of operations atomically
    async fn batch_write(
        &self,
        operations: Vec<crate::BatchOperation>,
    ) -> StorageResult<()>;

    /// Get optimal batch size for this backend
    fn optimal_batch_size(&self) -> usize {
        1000 // Default batch size
    }
}

/// Storage backend extension trait for advanced features
#[async_trait]
pub trait StorageBackendExt: StorageBackend {
    /// Create a snapshot of the current state
    async fn create_snapshot(&self) -> StorageResult<crate::StorageSnapshot>;

    /// Restore from a snapshot
    async fn restore_snapshot(
        &self,
        snapshot: crate::StorageSnapshot,
    ) -> StorageResult<()>;

    /// Export all data
    async fn export(&self) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Import data (replacing existing data)
    async fn import(
        &self,
        data: Vec<(StorageValue, StorageValue)>,
    ) -> StorageResult<()>;

    /// Get backend-specific configuration
    fn get_config(&self) -> StorageResult<crate::StorageBackendConfig>;

    /// Update backend-specific configuration
    async fn update_config(
        &self,
        config: crate::StorageBackendConfig,
    ) -> StorageResult<()>;
}

#[cfg(test)]
mod tests {
    use crate::StorageValue;

    #[test]
    fn test_storage_value_usage() {
        let key = StorageValue::from("test_key");
        let value = StorageValue::from("test_value");

        // Test that we can create operations with StorageValue
        assert_eq!(key.as_slice(), b"test_key");
        assert_eq!(value.as_slice(), b"test_value");
    }
}
