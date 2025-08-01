use crate::{StorageResult, StorageValue};
use async_trait::async_trait;

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

    /// Check if a key exists
    async fn exists(&self, key: &[u8]) -> StorageResult<bool>;

    /// Scan keys with a prefix
    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Scan keys in a range
    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Get the number of entries
    async fn count(&self) -> StorageResult<u64>;

    /// Flush any pending writes
    async fn flush(&self) -> StorageResult<()>;

    /// Close the storage backend
    async fn close(&self) -> StorageResult<()>;

    /// Get backend type information
    fn backend_type(&self) -> StorageBackendType;

    /// Get backend statistics
    async fn stats(&self) -> StorageResult<StorageStats>;

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

/// Storage backend types
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StorageBackendType {
    Memory,
    Redb,
    Fjall,
    RocksDb,
}

impl std::fmt::Display for StorageBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, "memory"),
            Self::Redb => write!(f, "redb"),
            Self::Fjall => write!(f, "fjall"),
            Self::RocksDb => write!(f, "rocksdb"),
        }
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub entries_count: u64,
    pub total_size_bytes: u64,
    pub memory_usage_bytes: Option<u64>,
    pub disk_usage_bytes: Option<u64>,
    pub cache_hit_rate: Option<f64>,
    pub backend_specific: std::collections::HashMap<String, String>,
}

impl Default for StorageStats {
    fn default() -> Self {
        Self {
            entries_count: 0,
            total_size_bytes: 0,
            memory_usage_bytes: None,
            disk_usage_bytes: None,
            cache_hit_rate: None,
            backend_specific: std::collections::HashMap::new(),
        }
    }
}

/// Batch operation for efficient bulk operations
#[derive(Debug, Clone)]
pub enum BatchOperation {
    Put {
        key: StorageValue,
        value: StorageValue,
    },
    Delete {
        key: StorageValue,
    },
}

/// Batch write trait for efficient bulk operations
#[async_trait]
pub trait BatchWrite {
    /// Execute a batch of operations atomically
    async fn batch_write(
        &self,
        operations: Vec<BatchOperation>,
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
    async fn create_snapshot(&self) -> StorageResult<StorageSnapshot>;

    /// Restore from a snapshot
    async fn restore_snapshot(
        &self,
        snapshot: StorageSnapshot,
    ) -> StorageResult<()>;

    /// Export all data
    async fn export(&self) -> StorageResult<Vec<(StorageValue, StorageValue)>>;

    /// Import data (replacing existing data)
    async fn import(
        &self,
        data: Vec<(StorageValue, StorageValue)>,
    ) -> StorageResult<()>;

    /// Get backend-specific configuration
    fn get_config(&self) -> StorageResult<StorageBackendConfig>;

    /// Update backend-specific configuration
    async fn update_config(
        &self,
        config: StorageBackendConfig,
    ) -> StorageResult<()>;
}

/// Storage snapshot for backup/restore operations
#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    pub timestamp: std::time::SystemTime,
    pub data: Vec<u8>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Backend-specific configuration
#[derive(Debug, Clone)]
pub struct StorageBackendConfig {
    pub backend_type: StorageBackendType,
    pub properties: std::collections::HashMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_display() {
        assert_eq!(StorageBackendType::Memory.to_string(), "memory");
        assert_eq!(StorageBackendType::Redb.to_string(), "redb");
        assert_eq!(StorageBackendType::Fjall.to_string(), "fjall");
        assert_eq!(StorageBackendType::RocksDb.to_string(), "rocksdb");
    }

    #[test]
    fn test_batch_operation() {
        let key = StorageValue::from("test_key");
        let value = StorageValue::from("test_value");

        let put_op = BatchOperation::Put {
            key: key.clone(),
            value: value.clone(),
        };
        let delete_op = BatchOperation::Delete { key };

        match put_op {
            BatchOperation::Put { key: _, value: _ } => (),
            _ => panic!("Expected Put operation"),
        }

        match delete_op {
            BatchOperation::Delete { key: _ } => (),
            _ => panic!("Expected Delete operation"),
        }
    }
}
