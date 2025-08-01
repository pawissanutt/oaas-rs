use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    StorageBackend, StorageBackendType, StorageConfig, StorageError, StorageResult,
    StorageStats, StorageTransaction, StorageValue,
};

/// In-memory storage backend implementation
#[derive(Debug)]
pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<Vec<u8>, StorageValue>>>,
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

impl MemoryStorage {
    /// Create a new memory storage backend
    pub async fn new(config: StorageConfig) -> StorageResult<Self> {
        if config.backend_type != StorageBackendType::Memory {
            return Err(StorageError::configuration(
                "Invalid backend type for MemoryStorage"
            ));
        }
        
        Ok(Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(StorageStats::default())),
        })
    }
    
    /// Update statistics
    async fn update_stats(&self) {
        let data = self.data.read().await;
        let mut stats = self.stats.write().await;
        
        stats.entries_count = data.len() as u64;
        stats.total_size_bytes = data.iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>() as u64;
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
    
    async fn put(&self, key: &[u8], value: StorageValue) -> StorageResult<()> {
        let mut data = self.data.write().await;
        
        // Check memory limit if configured
        if let Some(limit_mb) = self.config.memory_limit_mb {
            let current_size = data.iter()
                .map(|(k, v)| k.len() + v.len())
                .sum::<usize>();
            let new_size = current_size + key.len() + value.len();
            let limit_bytes = limit_mb * 1024 * 1024;
            
            if new_size > limit_bytes {
                return Err(StorageError::backend("Memory limit exceeded"));
            }
        }
        
        data.insert(key.to_vec(), value);
        drop(data);
        
        // Update stats asynchronously
        self.update_stats().await;
        Ok(())
    }
    
    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        let mut data = self.data.write().await;
        data.remove(key);
        drop(data);
        
        self.update_stats().await;
        Ok(())
    }
    
    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let data = self.data.read().await;
        Ok(data.contains_key(key))
    }
    
    async fn scan(&self, prefix: &[u8]) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        let data = self.data.read().await;
        let mut results = Vec::new();
        
        for (key, value) in data.iter() {
            if key.starts_with(prefix) {
                results.push((
                    StorageValue::from_slice(key),
                    value.clone(),
                ));
            }
        }
        
        // Sort by key for consistent ordering
        results.sort_by(|a, b| a.0.as_slice().cmp(b.0.as_slice()));
        Ok(results)
    }
    
    async fn scan_range(&self, start: &[u8], end: &[u8]) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        let data = self.data.read().await;
        let mut results = Vec::new();
        
        for (key, value) in data.iter() {
            if key.as_slice() >= start && key.as_slice() < end {
                results.push((
                    StorageValue::from_slice(key),
                    value.clone(),
                ));
            }
        }
        
        // Sort by key for consistent ordering
        results.sort_by(|a, b| a.0.as_slice().cmp(b.0.as_slice()));
        Ok(results)
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
        // Memory storage doesn't need compaction, but we can rebuild the HashMap
        // to potentially reduce memory fragmentation
        let mut data = self.data.write().await;
        let old_data: HashMap<Vec<u8>, StorageValue> = data.drain().collect();
        *data = old_data;
        Ok(())
    }
}

/// Memory storage transaction
pub struct MemoryTransaction {
    data: Arc<RwLock<HashMap<Vec<u8>, StorageValue>>>,
    operations: Vec<TransactionOperation>,
    committed: bool,
}

#[derive(Debug, Clone)]
enum TransactionOperation {
    Put { key: Vec<u8>, value: StorageValue },
    Delete { key: Vec<u8> },
}

impl MemoryTransaction {
    fn new(data: Arc<RwLock<HashMap<Vec<u8>, StorageValue>>>) -> Self {
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
                TransactionOperation::Put { key: op_key, value } if op_key == key => {
                    return Ok(Some(value.clone()));
                }
                TransactionOperation::Delete { key: op_key } if op_key == key => {
                    return Ok(None);
                }
                _ => continue,
            }
        }
        
        // If no pending operation, read from storage
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
    }
    
    async fn put(&mut self, key: &[u8], value: StorageValue) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction("Transaction already committed"));
        }
        
        self.operations.push(TransactionOperation::Put {
            key: key.to_vec(),
            value,
        });
        Ok(())
    }
    
    async fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction("Transaction already committed"));
        }
        
        self.operations.push(TransactionOperation::Delete {
            key: key.to_vec(),
        });
        Ok(())
    }
    
    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        Ok(self.get(key).await?.is_some())
    }
    
    async fn commit(mut self) -> StorageResult<()> {
        if self.committed {
            return Err(StorageError::transaction("Transaction already committed"));
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_memory_storage_basic_operations() {
        let config = StorageConfig::memory();
        let storage = MemoryStorage::new(config).await.unwrap();
        
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
        let storage = MemoryStorage::new(config).await.unwrap();
        
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
        let storage = MemoryStorage::new(config).await.unwrap();
        
        // Insert test data
        storage.put(b"prefix_1", StorageValue::from("value1")).await.unwrap();
        storage.put(b"prefix_2", StorageValue::from("value2")).await.unwrap();
        storage.put(b"other_3", StorageValue::from("value3")).await.unwrap();
        
        // Test prefix scan
        let results = storage.scan(b"prefix_").await.unwrap();
        assert_eq!(results.len(), 2);
        
        // Results should be sorted
        assert_eq!(results[0].0.as_slice(), b"prefix_1");
        assert_eq!(results[1].0.as_slice(), b"prefix_2");
    }
    
    #[tokio::test]
    async fn test_memory_limit() {
        let config = StorageConfig::memory().with_memory_limit(1); // 1MB limit
        let storage = MemoryStorage::new(config).await.unwrap();
        
        // Try to store more than 1MB of data
        let large_value = StorageValue::from(vec![0u8; 2 * 1024 * 1024]); // 2MB
        let result = storage.put(b"large_key", large_value).await;
        
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StorageError::Backend(_)));
    }
}
