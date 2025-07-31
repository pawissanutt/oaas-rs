# Storage Architecture for oprc-dp-storage

## Overview

This document details the storage backend architecture for the `oprc-dp-storage` common module. This module provides pluggable storage backends that can be used across all data plane components (ODGM, Gateway, Router, etc.). The storage layer is completely separated from replication logic, allowing for flexible storage backends that can work with any consistency model.

## Module Structure

```
commons/oprc-dp-storage/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── traits.rs          # StorageBackend, StorageTransaction traits
│   ├── factory.rs         # StorageFactory for creating backends
│   ├── error.rs           # StorageError types
│   ├── config.rs          # Configuration structs
│   ├── backends/
│   │   ├── mod.rs
│   │   ├── memory.rs      # In-memory storage implementation
│   │   ├── redb.rs        # Redb backend (feature: "redb")
│   │   ├── fjall.rs       # Fjall backend (feature: "fjall")
│   │   └── rocksdb.rs     # RocksDB backend (feature: "rocksdb")
│   ├── batch.rs           # Batch operations support
│   ├── migration.rs       # Data migration utilities
│   └── test_utils.rs      # Testing framework
```

## Storage Backend Abstraction

### Core Storage Trait

```rust
// src/storage/mod.rs
use std::error::Error;
use async_trait::async_trait;

#[async_trait]
pub trait StorageBackend: Send + Sync + Clone {
    type Error: Error + Send + Sync + 'static;
    type Transaction: StorageTransaction<Error = Self::Error>;
    
    /// Initialize the storage backend
    async fn initialize(&self) -> Result<(), Self::Error>;
    
    /// Begin a new transaction
    async fn begin_transaction(&self) -> Result<Self::Transaction, Self::Error>;
    
    /// Get a value by key
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;
    
    /// Put a key-value pair
    async fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    
    /// Delete a key
    async fn delete(&self, key: &[u8]) -> Result<(), Self::Error>;
    
    /// Scan keys with a prefix
    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Self::Error>;
    
    /// Check if a key exists
    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error>;
    
    /// Flush pending writes
    async fn flush(&self) -> Result<(), Self::Error>;
    
    /// Close the storage backend
    async fn close(&self) -> Result<(), Self::Error>;
    
    /// Get storage statistics
    async fn stats(&self) -> Result<StorageStats, Self::Error>;
    
    /// Compact the storage (if supported)
    async fn compact(&self) -> Result<(), Self::Error>;
    
    /// Get approximate size in bytes
    async fn size(&self) -> Result<u64, Self::Error>;
}

#[async_trait]
pub trait StorageTransaction: Send + Sync {
    type Error: Error + Send + Sync + 'static;
    
    /// Get a value by key within the transaction
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;
    
    /// Put a key-value pair within the transaction
    async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error>;
    
    /// Delete a key within the transaction
    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error>;
    
    /// Commit the transaction
    async fn commit(self) -> Result<(), Self::Error>;
    
    /// Rollback the transaction
    async fn rollback(self) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_keys: u64,
    pub total_size_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    pub delete_ops: u64,
    pub avg_read_latency_ms: f64,
    pub avg_write_latency_ms: f64,
}
```

### Storage Factory

```rust
// src/storage/factory.rs
use super::*;
use crate::config::StorageConfig;

pub struct StorageFactory;

impl StorageFactory {
    pub async fn create_storage(
        config: &StorageConfig,
    ) -> Result<Box<dyn StorageBackend<Error = StorageError>>, StorageError> {
        match config.backend_type {
            StorageBackendType::Memory => {
                let storage = MemoryStorage::new(
                    config.memory.clone().unwrap_or_default()
                )?;
                Ok(Box::new(storage))
            }
            #[cfg(feature = "redb")]
            StorageBackendType::Redb => {
                let storage = RedbStorage::new(
                    config.redb.as_ref().ok_or(StorageError::ConfigMissing)?
                ).await?;
                Ok(Box::new(storage))
            }
            #[cfg(feature = "fjall")]
            StorageBackendType::Fjall => {
                let storage = FjallStorage::new(
                    config.fjall.as_ref().ok_or(StorageError::ConfigMissing)?
                ).await?;
                Ok(Box::new(storage))
            }
            #[cfg(feature = "rocksdb")]
            StorageBackendType::RocksDb => {
                let storage = RocksDbStorage::new(
                    config.rocksdb.as_ref().ok_or(StorageError::ConfigMissing)?
                ).await?;
                Ok(Box::new(storage))
            }
        }
    }
}
```

## Storage Backend Implementations

### 1. Memory Storage Backend

```rust
// src/storage/memory.rs
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use super::*;

#[derive(Clone)]
pub struct MemoryStorage {
    data: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
    config: MemoryStorageConfig,
    stats: Arc<RwLock<StorageStats>>,
}

#[derive(Debug, Clone)]
pub struct MemoryStorageConfig {
    pub max_entries: Option<usize>,
    pub eviction_policy: EvictionPolicy,
    pub enable_compression: bool,
}

impl Default for MemoryStorageConfig {
    fn default() -> Self {
        Self {
            max_entries: None,
            eviction_policy: EvictionPolicy::LRU,
            enable_compression: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum EvictionPolicy {
    LRU,
    LFU,
    Random,
    None,
}

impl MemoryStorage {
    pub fn new(config: MemoryStorageConfig) -> Result<Self, StorageError> {
        Ok(Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            config,
            stats: Arc::new(RwLock::new(StorageStats::default())),
        })
    }
}

#[async_trait]
impl StorageBackend for MemoryStorage {
    type Error = StorageError;
    type Transaction = MemoryTransaction;
    
    async fn initialize(&self) -> Result<(), Self::Error> {
        // No initialization needed for memory storage
        Ok(())
    }
    
    async fn begin_transaction(&self) -> Result<Self::Transaction, Self::Error> {
        Ok(MemoryTransaction::new(self.data.clone()))
    }
    
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        let start = std::time::Instant::now();
        let data = self.data.read().await;
        let result = data.get(key).cloned();
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.read_ops += 1;
        stats.avg_read_latency_ms = 
            (stats.avg_read_latency_ms + start.elapsed().as_millis() as f64) / 2.0;
        
        Ok(result)
    }
    
    async fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        let start = std::time::Instant::now();
        let mut data = self.data.write().await;
        
        // Check max entries limit
        if let Some(max_entries) = self.config.max_entries {
            if data.len() >= max_entries && !data.contains_key(key) {
                self.evict_entry(&mut data).await;
            }
        }
        
        data.insert(key.to_vec(), value.to_vec());
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.write_ops += 1;
        stats.total_keys = data.len() as u64;
        stats.avg_write_latency_ms = 
            (stats.avg_write_latency_ms + start.elapsed().as_millis() as f64) / 2.0;
        
        Ok(())
    }
    
    async fn delete(&self, key: &[u8]) -> Result<(), Self::Error> {
        let mut data = self.data.write().await;
        data.remove(key);
        
        let mut stats = self.stats.write().await;
        stats.delete_ops += 1;
        stats.total_keys = data.len() as u64;
        
        Ok(())
    }
    
    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Self::Error> {
        let data = self.data.read().await;
        let results = data
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(results)
    }
    
    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        let data = self.data.read().await;
        Ok(data.contains_key(key))
    }
    
    async fn flush(&self) -> Result<(), Self::Error> {
        // No-op for memory storage
        Ok(())
    }
    
    async fn close(&self) -> Result<(), Self::Error> {
        let mut data = self.data.write().await;
        data.clear();
        Ok(())
    }
    
    async fn stats(&self) -> Result<StorageStats, Self::Error> {
        let stats = self.stats.read().await;
        Ok(stats.clone())
    }
    
    async fn compact(&self) -> Result<(), Self::Error> {
        // No-op for memory storage
        Ok(())
    }
    
    async fn size(&self) -> Result<u64, Self::Error> {
        let data = self.data.read().await;
        let size = data
            .iter()
            .map(|(k, v)| k.len() + v.len())
            .sum::<usize>() as u64;
        Ok(size)
    }
}

impl MemoryStorage {
    async fn evict_entry(&self, data: &mut HashMap<Vec<u8>, Vec<u8>>) {
        match self.config.eviction_policy {
            EvictionPolicy::Random => {
                if let Some(key) = data.keys().next().cloned() {
                    data.remove(&key);
                }
            }
            EvictionPolicy::LRU | EvictionPolicy::LFU => {
                // For simplicity, use random eviction
                // In production, implement proper LRU/LFU
                if let Some(key) = data.keys().next().cloned() {
                    data.remove(&key);
                }
            }
            EvictionPolicy::None => {
                // Don't evict
            }
        }
    }
}

pub struct MemoryTransaction {
    data: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>,
    changes: HashMap<Vec<u8>, TransactionOp>,
}

#[derive(Clone)]
enum TransactionOp {
    Put(Vec<u8>),
    Delete,
}

impl MemoryTransaction {
    pub fn new(data: Arc<RwLock<HashMap<Vec<u8>, Vec<u8>>>>) -> Self {
        Self {
            data,
            changes: HashMap::new(),
        }
    }
}

#[async_trait]
impl StorageTransaction for MemoryTransaction {
    type Error = StorageError;
    
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        // Check transaction changes first
        if let Some(op) = self.changes.get(key) {
            return Ok(match op {
                TransactionOp::Put(value) => Some(value.clone()),
                TransactionOp::Delete => None,
            });
        }
        
        // Fall back to main data
        let data = self.data.read().await;
        Ok(data.get(key).cloned())
    }
    
    async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.changes.insert(key.to_vec(), TransactionOp::Put(value.to_vec()));
        Ok(())
    }
    
    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error> {
        self.changes.insert(key.to_vec(), TransactionOp::Delete);
        Ok(())
    }
    
    async fn commit(self) -> Result<(), Self::Error> {
        let mut data = self.data.write().await;
        for (key, op) in self.changes {
            match op {
                TransactionOp::Put(value) => {
                    data.insert(key, value);
                }
                TransactionOp::Delete => {
                    data.remove(&key);
                }
            }
        }
        Ok(())
    }
    
    async fn rollback(self) -> Result<(), Self::Error> {
        // Just drop the changes
        Ok(())
    }
}
```

### 2. Redb Storage Backend

```rust
// src/storage/redb_backend.rs
#[cfg(feature = "redb")]
use redb::{Database, TableDefinition, ReadableTable, WriteTransaction};
use std::path::PathBuf;
use super::*;

const TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("data");

#[derive(Clone)]
pub struct RedbStorage {
    db: Arc<Database>,
    config: RedbConfig,
    stats: Arc<RwLock<StorageStats>>,
}

#[derive(Debug, Clone)]
pub struct RedbConfig {
    pub path: PathBuf,
    pub cache_size: Option<usize>,
    pub durability: RedbDurability,
    pub page_size: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum RedbDurability {
    Immediate,
    Eventual,
    Paranoid,
}

impl RedbStorage {
    pub async fn new(config: &RedbConfig) -> Result<Self, StorageError> {
        let db = Database::create(&config.path)
            .map_err(|e| StorageError::InitializationFailed(e.to_string()))?;
        
        // Initialize table
        let write_txn = db.begin_write()
            .map_err(|e| StorageError::InitializationFailed(e.to_string()))?;
        write_txn.open_table(TABLE)
            .map_err(|e| StorageError::InitializationFailed(e.to_string()))?;
        write_txn.commit()
            .map_err(|e| StorageError::InitializationFailed(e.to_string()))?;
        
        Ok(Self {
            db: Arc::new(db),
            config: config.clone(),
            stats: Arc::new(RwLock::new(StorageStats::default())),
        })
    }
}

#[async_trait]
impl StorageBackend for RedbStorage {
    type Error = StorageError;
    type Transaction = RedbTransaction;
    
    async fn initialize(&self) -> Result<(), Self::Error> {
        // Already initialized in new()
        Ok(())
    }
    
    async fn begin_transaction(&self) -> Result<Self::Transaction, Self::Error> {
        let txn = self.db.begin_write()
            .map_err(|e| StorageError::TransactionFailed(e.to_string()))?;
        Ok(RedbTransaction::new(txn))
    }
    
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        let start = std::time::Instant::now();
        
        let read_txn = self.db.begin_read()
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        let table = read_txn.open_table(TABLE)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        let result = table.get(key)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?
            .map(|guard| guard.value().to_vec());
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.read_ops += 1;
        stats.avg_read_latency_ms = 
            (stats.avg_read_latency_ms + start.elapsed().as_millis() as f64) / 2.0;
        
        Ok(result)
    }
    
    async fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        let start = std::time::Instant::now();
        
        let write_txn = self.db.begin_write()
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        {
            let mut table = write_txn.open_table(TABLE)
                .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
            table.insert(key, value)
                .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        }
        write_txn.commit()
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.write_ops += 1;
        stats.avg_write_latency_ms = 
            (stats.avg_write_latency_ms + start.elapsed().as_millis() as f64) / 2.0;
        
        Ok(())
    }
    
    async fn delete(&self, key: &[u8]) -> Result<(), Self::Error> {
        let write_txn = self.db.begin_write()
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        {
            let mut table = write_txn.open_table(TABLE)
                .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
            table.remove(key)
                .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        }
        write_txn.commit()
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        
        let mut stats = self.stats.write().await;
        stats.delete_ops += 1;
        
        Ok(())
    }
    
    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Self::Error> {
        let read_txn = self.db.begin_read()
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        let table = read_txn.open_table(TABLE)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        let mut results = Vec::new();
        let mut iter = table.iter()
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        while let Some(item) = iter.next() {
            let (key, value) = item
                .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
            let key_bytes = key.value();
            if key_bytes.starts_with(prefix) {
                results.push((key_bytes.to_vec(), value.value().to_vec()));
            }
        }
        
        Ok(results)
    }
    
    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        let read_txn = self.db.begin_read()
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        let table = read_txn.open_table(TABLE)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        Ok(table.get(key)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?
            .is_some())
    }
    
    async fn flush(&self) -> Result<(), Self::Error> {
        // Redb handles flushing automatically
        Ok(())
    }
    
    async fn close(&self) -> Result<(), Self::Error> {
        // Database will be closed when dropped
        Ok(())
    }
    
    async fn stats(&self) -> Result<StorageStats, Self::Error> {
        let stats = self.stats.read().await;
        Ok(stats.clone())
    }
    
    async fn compact(&self) -> Result<(), Self::Error> {
        // Redb handles compaction automatically
        Ok(())
    }
    
    async fn size(&self) -> Result<u64, Self::Error> {
        let metadata = std::fs::metadata(&self.config.path)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        Ok(metadata.len())
    }
}

pub struct RedbTransaction {
    txn: WriteTransaction,
}

impl RedbTransaction {
    pub fn new(txn: WriteTransaction) -> Self {
        Self { txn }
    }
}

#[async_trait]
impl StorageTransaction for RedbTransaction {
    type Error = StorageError;
    
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        let table = self.txn.open_table(TABLE)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        Ok(table.get(key)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?
            .map(|guard| guard.value().to_vec()))
    }
    
    async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        let mut table = self.txn.open_table(TABLE)
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        table.insert(key, value)
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(())
    }
    
    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error> {
        let mut table = self.txn.open_table(TABLE)
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        table.remove(key)
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        Ok(())
    }
    
    async fn commit(self) -> Result<(), Self::Error> {
        self.txn.commit()
            .map_err(|e| StorageError::TransactionFailed(e.to_string()))
    }
    
    async fn rollback(self) -> Result<(), Self::Error> {
        self.txn.abort()
            .map_err(|e| StorageError::TransactionFailed(e.to_string()))
    }
}
```

### 3. Fjall Storage Backend

```rust
// src/storage/fjall_backend.rs
#[cfg(feature = "fjall")]
use fjall::{Config, Keyspace, PartitionCreateOptions};
use std::path::PathBuf;
use super::*;

#[derive(Clone)]
pub struct FjallStorage {
    keyspace: Arc<Keyspace>,
    partition_name: String,
    config: FjallConfig,
    stats: Arc<RwLock<StorageStats>>,
}

#[derive(Debug, Clone)]
pub struct FjallConfig {
    pub path: PathBuf,
    pub block_cache_size: Option<usize>,
    pub compression: CompressionType,
    pub max_write_buffer_size: Option<usize>,
    pub max_memtables: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum CompressionType {
    None,
    Snappy,
    Lz4,
    Zstd,
}

impl FjallStorage {
    pub async fn new(config: &FjallConfig) -> Result<Self, StorageError> {
        let mut fjall_config = Config::new(&config.path);
        
        if let Some(cache_size) = config.block_cache_size {
            fjall_config = fjall_config.block_cache_size(cache_size);
        }
        
        if let Some(buffer_size) = config.max_write_buffer_size {
            fjall_config = fjall_config.max_write_buffer_size(buffer_size);
        }
        
        let keyspace = fjall_config.open()
            .map_err(|e| StorageError::InitializationFailed(e.to_string()))?;
        
        let partition_name = "data".to_string();
        let partition_options = PartitionCreateOptions::default();
        
        keyspace.open_partition(&partition_name, partition_options)
            .map_err(|e| StorageError::InitializationFailed(e.to_string()))?;
        
        Ok(Self {
            keyspace: Arc::new(keyspace),
            partition_name,
            config: config.clone(),
            stats: Arc::new(RwLock::new(StorageStats::default())),
        })
    }
}

#[async_trait]
impl StorageBackend for FjallStorage {
    type Error = StorageError;
    type Transaction = FjallTransaction;
    
    async fn initialize(&self) -> Result<(), Self::Error> {
        // Already initialized in new()
        Ok(())
    }
    
    async fn begin_transaction(&self) -> Result<Self::Transaction, Self::Error> {
        let partition = self.keyspace.open_partition(
            &self.partition_name, 
            PartitionCreateOptions::default()
        ).map_err(|e| StorageError::TransactionFailed(e.to_string()))?;
        
        Ok(FjallTransaction::new(partition))
    }
    
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        let start = std::time::Instant::now();
        
        let partition = self.keyspace.open_partition(
            &self.partition_name, 
            PartitionCreateOptions::default()
        ).map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        let result = partition.get(key)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?
            .map(|value| value.to_vec());
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.read_ops += 1;
        stats.avg_read_latency_ms = 
            (stats.avg_read_latency_ms + start.elapsed().as_millis() as f64) / 2.0;
        
        Ok(result)
    }
    
    async fn put(&self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        let start = std::time::Instant::now();
        
        let partition = self.keyspace.open_partition(
            &self.partition_name, 
            PartitionCreateOptions::default()
        ).map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        
        partition.insert(key, value)
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.write_ops += 1;
        stats.avg_write_latency_ms = 
            (stats.avg_write_latency_ms + start.elapsed().as_millis() as f64) / 2.0;
        
        Ok(())
    }
    
    async fn delete(&self, key: &[u8]) -> Result<(), Self::Error> {
        let partition = self.keyspace.open_partition(
            &self.partition_name, 
            PartitionCreateOptions::default()
        ).map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        
        partition.remove(key)
            .map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        
        let mut stats = self.stats.write().await;
        stats.delete_ops += 1;
        
        Ok(())
    }
    
    async fn scan(&self, prefix: &[u8]) -> Result<Vec<(Vec<u8>, Vec<u8>)>, Self::Error> {
        let partition = self.keyspace.open_partition(
            &self.partition_name, 
            PartitionCreateOptions::default()
        ).map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        let mut results = Vec::new();
        for item in partition.prefix(prefix) {
            let (key, value) = item
                .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
            results.push((key.to_vec(), value.to_vec()));
        }
        
        Ok(results)
    }
    
    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        let partition = self.keyspace.open_partition(
            &self.partition_name, 
            PartitionCreateOptions::default()
        ).map_err(|e| StorageError::ReadFailed(e.to_string()))?;
        
        Ok(partition.contains_key(key)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?)
    }
    
    async fn flush(&self) -> Result<(), Self::Error> {
        self.keyspace.flush()
            .map_err(|e| StorageError::WriteFailed(e.to_string()))
    }
    
    async fn close(&self) -> Result<(), Self::Error> {
        // Keyspace will be closed when dropped
        Ok(())
    }
    
    async fn stats(&self) -> Result<StorageStats, Self::Error> {
        let stats = self.stats.read().await;
        Ok(stats.clone())
    }
    
    async fn compact(&self) -> Result<(), Self::Error> {
        let partition = self.keyspace.open_partition(
            &self.partition_name, 
            PartitionCreateOptions::default()
        ).map_err(|e| StorageError::WriteFailed(e.to_string()))?;
        
        partition.compact()
            .map_err(|e| StorageError::WriteFailed(e.to_string()))
    }
    
    async fn size(&self) -> Result<u64, Self::Error> {
        // Get approximate size from filesystem
        let mut total_size = 0u64;
        for entry in std::fs::read_dir(&self.config.path)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))? 
        {
            let entry = entry.map_err(|e| StorageError::ReadFailed(e.to_string()))?;
            let metadata = entry.metadata()
                .map_err(|e| StorageError::ReadFailed(e.to_string()))?;
            total_size += metadata.len();
        }
        Ok(total_size)
    }
}

pub struct FjallTransaction {
    partition: fjall::Partition,
    changes: HashMap<Vec<u8>, TransactionOp>,
}

impl FjallTransaction {
    pub fn new(partition: fjall::Partition) -> Self {
        Self {
            partition,
            changes: HashMap::new(),
        }
    }
}

#[async_trait]
impl StorageTransaction for FjallTransaction {
    type Error = StorageError;
    
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        // Check transaction changes first
        if let Some(op) = self.changes.get(key) {
            return Ok(match op {
                TransactionOp::Put(value) => Some(value.clone()),
                TransactionOp::Delete => None,
            });
        }
        
        // Fall back to partition
        Ok(self.partition.get(key)
            .map_err(|e| StorageError::ReadFailed(e.to_string()))?
            .map(|value| value.to_vec()))
    }
    
    async fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.changes.insert(key.to_vec(), TransactionOp::Put(value.to_vec()));
        Ok(())
    }
    
    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error> {
        self.changes.insert(key.to_vec(), TransactionOp::Delete);
        Ok(())
    }
    
    async fn commit(self) -> Result<(), Self::Error> {
        for (key, op) in self.changes {
            match op {
                TransactionOp::Put(value) => {
                    self.partition.insert(&key, &value)
                        .map_err(|e| StorageError::TransactionFailed(e.to_string()))?;
                }
                TransactionOp::Delete => {
                    self.partition.remove(&key)
                        .map_err(|e| StorageError::TransactionFailed(e.to_string()))?;
                }
            }
        }
        Ok(())
    }
    
    async fn rollback(self) -> Result<(), Self::Error> {
        // Just drop the changes
        Ok(())
    }
}
```

## Error Handling

```rust
// src/storage/error.rs
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum StorageError {
    #[error("Storage initialization failed: {0}")]
    InitializationFailed(String),
    
    #[error("Read operation failed: {0}")]
    ReadFailed(String),
    
    #[error("Write operation failed: {0}")]
    WriteFailed(String),
    
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
    
    #[error("Storage configuration missing")]
    ConfigMissing,
    
    #[error("Storage backend not supported")]
    BackendNotSupported,
    
    #[error("Storage is full")]
    StorageFull,
    
    #[error("Key not found")]
    KeyNotFound,
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("IO error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for StorageError {
    fn from(err: std::io::Error) -> Self {
        StorageError::IoError(err.to_string())
    }
}
```

## Storage Configuration

```rust
// src/storage/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub backend_type: StorageBackendType,
    pub memory: Option<MemoryStorageConfig>,
    
    #[cfg(feature = "redb")]
    pub redb: Option<RedbConfig>,
    
    #[cfg(feature = "fjall")]
    pub fjall: Option<FjallConfig>,
    
    #[cfg(feature = "rocksdb")]
    pub rocksdb: Option<RocksDbConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageBackendType {
    Memory,
    
    #[cfg(feature = "redb")]
    Redb,
    
    #[cfg(feature = "fjall")]
    Fjall,
    
    #[cfg(feature = "rocksdb")]
    RocksDb,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend_type: StorageBackendType::Memory,
            memory: Some(MemoryStorageConfig::default()),
            
            #[cfg(feature = "redb")]
            redb: None,
            
            #[cfg(feature = "fjall")]
            fjall: None,
            
            #[cfg(feature = "rocksdb")]
            rocksdb: None,
        }
    }
}
```

## Performance Optimizations

### Batch Operations

```rust
// src/storage/batch.rs
use super::*;

#[async_trait]
pub trait BatchStorageBackend: StorageBackend {
    async fn batch_get(&self, keys: &[&[u8]]) -> Result<Vec<Option<Vec<u8>>>, Self::Error>;
    async fn batch_put(&self, items: &[(&[u8], &[u8])]) -> Result<(), Self::Error>;
    async fn batch_delete(&self, keys: &[&[u8]]) -> Result<(), Self::Error>;
}

pub struct StorageBatch<S: StorageBackend> {
    storage: S,
    operations: Vec<BatchOperation>,
}

#[derive(Clone)]
enum BatchOperation {
    Put(Vec<u8>, Vec<u8>),
    Delete(Vec<u8>),
}

impl<S: StorageBackend> StorageBatch<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            operations: Vec::new(),
        }
    }
    
    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        self.operations.push(BatchOperation::Put(
            key.to_vec(), 
            value.to_vec()
        ));
    }
    
    pub fn delete(&mut self, key: &[u8]) {
        self.operations.push(BatchOperation::Delete(key.to_vec()));
    }
    
    pub async fn commit(self) -> Result<(), S::Error> {
        let txn = self.storage.begin_transaction().await?;
        let mut txn = txn;
        
        for op in self.operations {
            match op {
                BatchOperation::Put(key, value) => {
                    txn.put(&key, &value).await?;
                }
                BatchOperation::Delete(key) => {
                    txn.delete(&key).await?;
                }
            }
        }
        
        txn.commit().await
    }
}
```

## Testing Framework

```rust
// src/storage/test_utils.rs
#[cfg(test)]
use super::*;
use tokio_test;

pub async fn test_storage_backend<S: StorageBackend>(storage: S) 
where 
    S::Error: std::fmt::Debug,
{
    // Initialize
    storage.initialize().await.unwrap();
    
    // Test basic operations
    test_basic_operations(&storage).await;
    test_transaction_support(&storage).await;
    test_scan_operations(&storage).await;
    test_batch_operations(&storage).await;
    
    // Cleanup
    storage.close().await.unwrap();
}

async fn test_basic_operations<S: StorageBackend>(storage: &S) 
where 
    S::Error: std::fmt::Debug,
{
    let key = b"test_key";
    let value = b"test_value";
    
    // Test put/get
    storage.put(key, value).await.unwrap();
    let result = storage.get(key).await.unwrap();
    assert_eq!(result, Some(value.to_vec()));
    
    // Test exists
    assert!(storage.exists(key).await.unwrap());
    
    // Test delete
    storage.delete(key).await.unwrap();
    let result = storage.get(key).await.unwrap();
    assert_eq!(result, None);
    
    // Test exists after delete
    assert!(!storage.exists(key).await.unwrap());
}

async fn test_transaction_support<S: StorageBackend>(storage: &S) 
where 
    S::Error: std::fmt::Debug,
{
    let mut txn = storage.begin_transaction().await.unwrap();
    
    let key = b"txn_key";
    let value = b"txn_value";
    
    // Put in transaction
    txn.put(key, value).await.unwrap();
    
    // Should not be visible outside transaction yet
    assert_eq!(storage.get(key).await.unwrap(), None);
    
    // Should be visible within transaction
    assert_eq!(txn.get(key).await.unwrap(), Some(value.to_vec()));
    
    // Commit transaction
    txn.commit().await.unwrap();
    
    // Should now be visible outside transaction
    assert_eq!(storage.get(key).await.unwrap(), Some(value.to_vec()));
}

// Additional test functions...
```

This storage architecture provides:

1. **Clean Abstractions**: Well-defined traits for storage backends and transactions
2. **Multiple Implementations**: Memory, Redb, Fjall, and RocksDB support
3. **Transaction Support**: ACID transactions across all backends
4. **Performance Monitoring**: Built-in statistics and metrics
5. **Error Handling**: Comprehensive error types and handling
6. **Testing Framework**: Standardized testing for all storage backends
7. **Batch Operations**: Optimized batch processing support
8. **Configuration**: Flexible configuration system for each backend

The storage layer is completely independent of replication concerns, making it easy to plug into any consistency model or replication strategy.
