//! Specialized storage implementations that bridge the specialized traits
//! to existing storage backends for enhanced storage architecture.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;

use crate::{
    ApplicationDataStorage, ApplicationReadTransaction,
    ApplicationWriteTransaction, CompressionType, IndexKeyExtractor,
    IndexQuery, RaftLogEntry, RaftLogEntryType, RaftLogStats, RaftLogStorage,
    RaftSnapshot, RaftSnapshotStorage, RetentionStrategy, SnapshotMetadata,
    SnapshotRetentionPolicy, SnapshotStream, SpecializedStorageError,
    StorageBackend, StorageResult, StorageValue,
};

// ============================================================================
// Raft Log Storage Implementation
// ============================================================================

/// Append-only log storage implementation using a StorageBackend
#[derive(Debug, Clone)]
pub struct AppendOnlyLogStorage<B: StorageBackend> {
    backend: B,
    log_prefix: Vec<u8>,
    metadata: Arc<RwLock<LogMetadata>>,
}

#[derive(Debug, Clone)]
struct LogMetadata {
    first_index: Option<u64>,
    last_index: Option<u64>,
    entry_count: u64,
}

impl<B: StorageBackend + Clone> AppendOnlyLogStorage<B> {
    pub async fn new(backend: B) -> Result<Self, SpecializedStorageError> {
        let log_prefix = b"raft_log_".to_vec();
        let metadata = Arc::new(RwLock::new(LogMetadata {
            first_index: None,
            last_index: None,
            entry_count: 0,
        }));

        let storage = Self {
            backend,
            log_prefix,
            metadata,
        };

        // Initialize metadata from existing entries
        storage.rebuild_metadata().await?;

        Ok(storage)
    }

    fn entry_key(&self, index: u64) -> Vec<u8> {
        let mut key = self.log_prefix.clone();
        key.extend_from_slice(&index.to_be_bytes());
        key
    }

    async fn rebuild_metadata(&self) -> Result<(), SpecializedStorageError> {
        let entries = self
            .backend
            .scan(&self.log_prefix)
            .await
            .map_err(SpecializedStorageError::Backend)?;

        let mut metadata = self.metadata.write().await;

        if entries.is_empty() {
            metadata.first_index = None;
            metadata.last_index = None;
            metadata.entry_count = 0;
        } else {
            let mut indices: Vec<u64> = entries
                .iter()
                .filter_map(|(key, _)| {
                    if key.len() >= self.log_prefix.len() + 8 {
                        let index_bytes = &key
                            [self.log_prefix.len()..self.log_prefix.len() + 8];
                        Some(u64::from_be_bytes(index_bytes.try_into().ok()?))
                    } else {
                        None
                    }
                })
                .collect();

            indices.sort_unstable();
            metadata.first_index = indices.first().copied();
            metadata.last_index = indices.last().copied();
            metadata.entry_count = indices.len() as u64;
        }

        Ok(())
    }
}

#[async_trait]
impl<B: StorageBackend + Clone + 'static> RaftLogStorage
    for AppendOnlyLogStorage<B>
{
    type Error = SpecializedStorageError;

    async fn append_entry(
        &self,
        index: u64,
        term: u64,
        entry: &[u8],
    ) -> Result<(), Self::Error> {
        let log_entry = RaftLogEntry {
            index,
            term,
            entry_type: RaftLogEntryType::Normal,
            data: entry.to_vec(),
            timestamp: SystemTime::now(),
            checksum: None,
        };

        let serialized = bincode::serde::encode_to_vec(
            &log_entry,
            bincode::config::standard(),
        )
        .map_err(|e| {
            SpecializedStorageError::RaftLog(format!(
                "Serialization error: {}",
                e
            ))
        })?;

        let key = self.entry_key(index);
        self.backend
            .put(&key, StorageValue::from(serialized))
            .await
            .map_err(SpecializedStorageError::Backend)?;

        // Update metadata
        let mut metadata = self.metadata.write().await;
        if metadata.last_index.map_or(true, |last| index > last) {
            metadata.last_index = Some(index);
        }
        if metadata.first_index.is_none() {
            metadata.first_index = Some(index);
        }
        metadata.entry_count += 1;

        Ok(())
    }

    async fn append_entries(
        &self,
        entries: Vec<RaftLogEntry>,
    ) -> Result<(), Self::Error> {
        for entry in entries {
            let serialized = bincode::serde::encode_to_vec(
                &entry,
                bincode::config::standard(),
            )
            .map_err(|e| {
                SpecializedStorageError::RaftLog(format!(
                    "Serialization error: {}",
                    e
                ))
            })?;

            let key = self.entry_key(entry.index);
            self.backend
                .put(&key, StorageValue::from(serialized))
                .await
                .map_err(SpecializedStorageError::Backend)?;
        }

        self.rebuild_metadata().await?;
        Ok(())
    }

    async fn get_entry(
        &self,
        index: u64,
    ) -> Result<Option<RaftLogEntry>, Self::Error> {
        let key = self.entry_key(index);
        let data = self
            .backend
            .get(&key)
            .await
            .map_err(SpecializedStorageError::Backend)?;

        match data {
            Some(data) => {
                let (entry, _): (RaftLogEntry, usize) =
                    bincode::serde::decode_from_slice(
                        data.as_slice(),
                        bincode::config::standard(),
                    )
                    .map_err(|e| {
                        SpecializedStorageError::RaftLog(format!(
                            "Deserialization error: {}",
                            e
                        ))
                    })?;
                Ok(Some(entry))
            }
            None => Ok(None),
        }
    }

    async fn get_entries(
        &self,
        start: u64,
        end: u64,
    ) -> Result<Vec<RaftLogEntry>, Self::Error> {
        let mut entries = Vec::new();

        for index in start..end {
            if let Some(entry) = self.get_entry(index).await? {
                entries.push(entry);
            }
        }

        Ok(entries)
    }

    async fn last_index(&self) -> Result<Option<u64>, Self::Error> {
        let metadata = self.metadata.read().await;
        Ok(metadata.last_index)
    }

    async fn first_index(&self) -> Result<Option<u64>, Self::Error> {
        let metadata = self.metadata.read().await;
        Ok(metadata.first_index)
    }

    async fn truncate_from(&self, index: u64) -> Result<(), Self::Error> {
        let metadata = self.metadata.read().await;
        if let Some(last_index) = metadata.last_index {
            for i in index..=last_index {
                let key = self.entry_key(i);
                self.backend
                    .delete(&key)
                    .await
                    .map_err(SpecializedStorageError::Backend)?;
            }
        }
        drop(metadata);

        self.rebuild_metadata().await?;
        Ok(())
    }

    async fn truncate_before(&self, index: u64) -> Result<(), Self::Error> {
        let metadata = self.metadata.read().await;
        if let Some(first_index) = metadata.first_index {
            for i in first_index..index {
                let key = self.entry_key(i);
                self.backend
                    .delete(&key)
                    .await
                    .map_err(SpecializedStorageError::Backend)?;
            }
        }
        drop(metadata);

        self.rebuild_metadata().await?;
        Ok(())
    }

    async fn log_stats(&self) -> Result<RaftLogStats, Self::Error> {
        let metadata = self.metadata.read().await;
        let backend_stats = self
            .backend
            .stats()
            .await
            .map_err(SpecializedStorageError::Backend)?;

        Ok(RaftLogStats {
            first_index: metadata.first_index,
            last_index: metadata.last_index,
            entry_count: metadata.entry_count,
            total_size_bytes: backend_stats.total_size_bytes,
            compacted_entries: 0, // Would track this separately
            write_throughput_ops_per_sec: 0.0, // Would measure this
            avg_entry_size_bytes: if metadata.entry_count > 0 {
                backend_stats.total_size_bytes as f64
                    / metadata.entry_count as f64
            } else {
                0.0
            },
        })
    }

    async fn sync(&self) -> Result<(), Self::Error> {
        self.backend
            .flush()
            .await
            .map_err(SpecializedStorageError::Backend)
    }
}

// ============================================================================
// Raft Snapshot Storage Implementation
// ============================================================================

/// Compressed snapshot storage implementation
#[derive(Debug, Clone)]
pub struct CompressedSnapshotStorage<B: StorageBackend> {
    backend: B,
    snapshot_prefix: Vec<u8>,
    compression: CompressionType,
}

impl<B: StorageBackend + Clone> CompressedSnapshotStorage<B> {
    pub async fn new(
        backend: B,
        compression: CompressionType,
    ) -> Result<Self, SpecializedStorageError> {
        Ok(Self {
            backend,
            snapshot_prefix: b"snapshot_".to_vec(),
            compression,
        })
    }

    fn snapshot_key(&self, snapshot_id: &str) -> Vec<u8> {
        let mut key = self.snapshot_prefix.clone();
        key.extend_from_slice(snapshot_id.as_bytes());
        key
    }

    fn metadata_key(&self, snapshot_id: &str) -> Vec<u8> {
        let mut key = self.snapshot_prefix.clone();
        key.extend_from_slice(b"meta_");
        key.extend_from_slice(snapshot_id.as_bytes());
        key
    }

    async fn compress_data(
        &self,
        data: &[u8],
    ) -> Result<Vec<u8>, SpecializedStorageError> {
        match self.compression {
            CompressionType::None => Ok(data.to_vec()),
            _ => {
                // For now, just return uncompressed data to avoid external dependencies
                // In a full implementation, we would use flate2, lz4, zstd, etc.
                tracing::warn!("Compression requested but not implemented, storing uncompressed");
                Ok(data.to_vec())
            }
        }
    }

    async fn decompress_data(
        &self,
        data: &[u8],
    ) -> Result<Vec<u8>, SpecializedStorageError> {
        match self.compression {
            CompressionType::None => Ok(data.to_vec()),
            _ => {
                // For now, just return data as-is since we're not actually compressing
                tracing::warn!("Decompression requested but not implemented, returning data as-is");
                Ok(data.to_vec())
            }
        }
    }
}

#[async_trait]
impl<B: StorageBackend + Clone + 'static> RaftSnapshotStorage
    for CompressedSnapshotStorage<B>
{
    type Error = SpecializedStorageError;

    async fn create_snapshot(
        &self,
        mut snapshot: RaftSnapshot,
    ) -> Result<String, Self::Error> {
        let compressed_data = self.compress_data(&snapshot.data).await?;

        // Update metadata with compression info
        snapshot.metadata.compressed_size_bytes = compressed_data.len() as u64;
        snapshot.metadata.size_bytes = snapshot.data.len() as u64;
        snapshot.metadata.compression = self.compression.clone();

        // Store compressed data
        let data_key = self.snapshot_key(&snapshot.id);
        self.backend
            .put(&data_key, StorageValue::from(compressed_data))
            .await
            .map_err(SpecializedStorageError::Backend)?;

        // Store metadata
        let metadata_key = self.metadata_key(&snapshot.id);
        let metadata_serialized = bincode::serde::encode_to_vec(
            &snapshot.metadata,
            bincode::config::standard(),
        )
        .map_err(|e| {
            SpecializedStorageError::Snapshot(format!(
                "Metadata serialization error: {}",
                e
            ))
        })?;
        self.backend
            .put(&metadata_key, StorageValue::from(metadata_serialized))
            .await
            .map_err(SpecializedStorageError::Backend)?;

        Ok(snapshot.id)
    }

    async fn get_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<Option<RaftSnapshot>, Self::Error> {
        // Get metadata first
        let metadata_key = self.metadata_key(snapshot_id);
        let metadata_data = self
            .backend
            .get(&metadata_key)
            .await
            .map_err(SpecializedStorageError::Backend)?;

        let metadata = match metadata_data {
            Some(data) => {
                let (metadata, _): (SnapshotMetadata, usize) =
                    bincode::serde::decode_from_slice(
                        data.as_slice(),
                        bincode::config::standard(),
                    )
                    .map_err(|e| {
                        SpecializedStorageError::Snapshot(format!(
                            "Metadata deserialization error: {}",
                            e
                        ))
                    })?;
                metadata
            }
            None => return Ok(None),
        };

        // Get compressed data
        let data_key = self.snapshot_key(snapshot_id);
        let compressed_data = self
            .backend
            .get(&data_key)
            .await
            .map_err(SpecializedStorageError::Backend)?;

        let data = match compressed_data {
            Some(compressed) => {
                self.decompress_data(compressed.as_slice()).await?
            }
            None => return Ok(None),
        };

        Ok(Some(RaftSnapshot {
            id: snapshot_id.to_string(),
            last_included_index: metadata.last_included_index,
            last_included_term: metadata.last_included_term,
            membership_config: Vec::new(), // Would be stored separately
            data,
            metadata,
        }))
    }

    async fn list_snapshots(
        &self,
    ) -> Result<Vec<SnapshotMetadata>, Self::Error> {
        let metadata_prefix = {
            let mut prefix = self.snapshot_prefix.clone();
            prefix.extend_from_slice(b"meta_");
            prefix
        };

        let entries = self
            .backend
            .scan(&metadata_prefix)
            .await
            .map_err(SpecializedStorageError::Backend)?;

        let mut snapshots = Vec::new();
        for (_, data) in entries {
            let (metadata, _): (SnapshotMetadata, usize) =
                bincode::serde::decode_from_slice(
                    data.as_slice(),
                    bincode::config::standard(),
                )
                .map_err(|e| {
                    SpecializedStorageError::Snapshot(format!(
                        "Metadata deserialization error: {}",
                        e
                    ))
                })?;
            snapshots.push(metadata);
        }

        // Sort by creation time (newest first)
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(snapshots)
    }

    async fn delete_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<(), Self::Error> {
        let data_key = self.snapshot_key(snapshot_id);
        let metadata_key = self.metadata_key(snapshot_id);

        self.backend
            .delete(&data_key)
            .await
            .map_err(SpecializedStorageError::Backend)?;
        self.backend
            .delete(&metadata_key)
            .await
            .map_err(SpecializedStorageError::Backend)?;

        Ok(())
    }

    async fn stream_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<SnapshotStream, Self::Error> {
        // For simple implementation, we'll load the entire snapshot
        let snapshot =
            self.get_snapshot(snapshot_id).await?.ok_or_else(|| {
                SpecializedStorageError::Snapshot(
                    "Snapshot not found".to_string(),
                )
            })?;

        Ok(SnapshotStream {
            metadata: snapshot.metadata,
            data: snapshot.data,
        })
    }

    async fn install_snapshot(
        &self,
        stream: SnapshotStream,
    ) -> Result<String, Self::Error> {
        let snapshot = RaftSnapshot {
            id: stream.metadata.id.clone(),
            last_included_index: stream.metadata.last_included_index,
            last_included_term: stream.metadata.last_included_term,
            membership_config: Vec::new(),
            data: stream.data,
            metadata: stream.metadata,
        };

        self.create_snapshot(snapshot).await
    }

    async fn latest_snapshot(
        &self,
    ) -> Result<Option<SnapshotMetadata>, Self::Error> {
        let snapshots = self.list_snapshots().await?;
        Ok(snapshots.into_iter().next())
    }

    async fn cleanup_snapshots(
        &self,
        retention: SnapshotRetentionPolicy,
    ) -> Result<u64, Self::Error> {
        let snapshots = self.list_snapshots().await?;
        let mut deleted_count = 0;

        match retention.strategy {
            RetentionStrategy::KeepLatest => {
                for snapshot in snapshots.into_iter().skip(retention.keep_count)
                {
                    self.delete_snapshot(&snapshot.id).await?;
                    deleted_count += 1;
                }
            }
            RetentionStrategy::KeepByAge => {
                let cutoff = SystemTime::now() - retention.max_age;
                for snapshot in snapshots {
                    if snapshot.created_at < cutoff {
                        self.delete_snapshot(&snapshot.id).await?;
                        deleted_count += 1;
                    }
                }
            }
            RetentionStrategy::Combined => {
                let cutoff = SystemTime::now() - retention.max_age;
                for (i, snapshot) in snapshots.into_iter().enumerate() {
                    if i >= retention.keep_count && snapshot.created_at < cutoff
                    {
                        self.delete_snapshot(&snapshot.id).await?;
                        deleted_count += 1;
                    }
                }
            }
            RetentionStrategy::SizeBasedLru => {
                if let Some(max_size) = retention.max_total_size {
                    let mut total_size = 0u64;
                    for snapshot in snapshots {
                        total_size += snapshot.compressed_size_bytes;
                        if total_size > max_size {
                            self.delete_snapshot(&snapshot.id).await?;
                            deleted_count += 1;
                        }
                    }
                }
            }
        }

        Ok(deleted_count)
    }

    async fn verify_snapshot(
        &self,
        snapshot_id: &str,
    ) -> Result<bool, Self::Error> {
        // Basic verification - check if snapshot exists and can be loaded
        match self.get_snapshot(snapshot_id).await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(_) => Ok(false),
        }
    }
}

// ============================================================================
// Enhanced Application Data Storage Implementation
// ============================================================================

/// Enhanced application data storage that wraps a StorageBackend with additional features
#[derive(Clone)]
pub struct EnhancedApplicationStorage<B: StorageBackend> {
    backend: B,
    indexes: Arc<RwLock<HashMap<String, IndexKeyExtractor>>>,
}

impl<B: StorageBackend + Clone> EnhancedApplicationStorage<B> {
    pub async fn new(backend: B) -> Result<Self, SpecializedStorageError> {
        Ok(Self {
            backend,
            indexes: Arc::new(RwLock::new(HashMap::new())),
        })
    }
}

#[async_trait]
impl<B: StorageBackend + Clone + 'static> StorageBackend
    for EnhancedApplicationStorage<B>
{
    type Transaction = B::Transaction;

    async fn begin_transaction(&self) -> StorageResult<Self::Transaction> {
        self.backend.begin_transaction().await
    }

    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        self.backend.get(key).await
    }

    async fn put(&self, key: &[u8], value: StorageValue) -> StorageResult<()> {
        self.backend.put(key, value).await
    }

    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        self.backend.delete(key).await
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        self.backend.exists(key).await
    }

    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        self.backend.scan(prefix).await
    }

    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        self.backend.scan_range(start, end).await
    }

    async fn count(&self) -> StorageResult<u64> {
        self.backend.count().await
    }

    async fn flush(&self) -> StorageResult<()> {
        self.backend.flush().await
    }

    async fn close(&self) -> StorageResult<()> {
        self.backend.close().await
    }

    fn backend_type(&self) -> crate::StorageBackendType {
        self.backend.backend_type()
    }

    async fn stats(&self) -> StorageResult<crate::StorageStats> {
        self.backend.stats().await
    }

    async fn compact(&self) -> StorageResult<()> {
        self.backend.compact().await
    }
}

// Simple read transaction wrapper
pub struct EnhancedReadTransaction<T> {
    transaction: T,
}

#[async_trait]
impl<T: crate::StorageTransaction + 'static> ApplicationReadTransaction
    for EnhancedReadTransaction<T>
{
    type Error = crate::StorageError;

    async fn get(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, Self::Error> {
        self.transaction.get(key).await
    }

    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, Self::Error> {
        let mut results = Vec::new();
        for key in keys {
            results.push(self.get(key).await?);
        }
        Ok(results)
    }

    async fn scan_range(
        &self,
        _start: &[u8],
        _end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error> {
        // For simplicity, we'll use the backend's scan_range through get operations
        // A full implementation would need access to the backend
        Err(crate::StorageError::InvalidOperation(
            "Scan range not implemented for transaction".to_string(),
        ))
    }

    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        self.transaction.exists(key).await
    }
}

// Enhanced write transaction wrapper
pub struct EnhancedWriteTransaction<T> {
    transaction: T,
}

#[async_trait]
impl<T: crate::StorageTransaction + 'static> ApplicationReadTransaction
    for EnhancedWriteTransaction<T>
{
    type Error = crate::StorageError;

    async fn get(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, Self::Error> {
        self.transaction.get(key).await
    }

    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, Self::Error> {
        let mut results = Vec::new();
        for key in keys {
            results.push(self.get(key).await?);
        }
        Ok(results)
    }

    async fn scan_range(
        &self,
        _start: &[u8],
        _end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error> {
        Err(crate::StorageError::InvalidOperation(
            "Scan range not implemented for transaction".to_string(),
        ))
    }

    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        self.transaction.exists(key).await
    }
}

#[async_trait]
impl<T: crate::StorageTransaction + 'static> ApplicationWriteTransaction
    for EnhancedWriteTransaction<T>
{
    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> Result<(), Self::Error> {
        self.transaction.put(key, value).await
    }

    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error> {
        self.transaction.delete(key).await
    }

    async fn compare_and_swap(
        &mut self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, Self::Error> {
        // Simple CAS implementation
        let current = self.get(key).await?;
        let matches = match (current.as_ref(), expected) {
            (Some(current), Some(expected)) => current.as_slice() == expected,
            (None, None) => true,
            _ => false,
        };

        if matches {
            match new_value {
                Some(value) => self.put(key, value).await?,
                None => self.delete(key).await?,
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn commit(self) -> Result<(), Self::Error> {
        self.transaction.commit().await
    }

    async fn rollback(self) -> Result<(), Self::Error> {
        self.transaction.rollback().await
    }
}

#[async_trait]
impl<B: StorageBackend + Clone + 'static> ApplicationDataStorage
    for EnhancedApplicationStorage<B>
{
    type ReadTransaction = EnhancedReadTransaction<B::Transaction>;
    type WriteTransaction = EnhancedWriteTransaction<B::Transaction>;

    async fn begin_read_transaction(
        &self,
    ) -> Result<Self::ReadTransaction, crate::StorageError> {
        let transaction =
            self.backend.begin_transaction().await.map_err(|e| {
                crate::StorageError::Backend(format!(
                    "Transaction error: {}",
                    e
                ))
            })?;
        Ok(EnhancedReadTransaction { transaction })
    }

    async fn begin_write_transaction(
        &self,
    ) -> Result<Self::WriteTransaction, crate::StorageError> {
        let transaction =
            self.backend.begin_transaction().await.map_err(|e| {
                crate::StorageError::Backend(format!(
                    "Transaction error: {}",
                    e
                ))
            })?;
        Ok(EnhancedWriteTransaction { transaction })
    }

    async fn scan_range_paginated(
        &self,
        start: &[u8],
        end: &[u8],
        limit: Option<usize>,
    ) -> Result<
        (Vec<(StorageValue, StorageValue)>, Option<StorageValue>),
        crate::StorageError,
    > {
        let mut results = self.backend.scan_range(start, end).await?;

        let next_cursor = if let Some(limit) = limit {
            if results.len() > limit {
                let cursor = results[limit].0.clone();
                results.truncate(limit);
                Some(cursor)
            } else {
                None
            }
        } else {
            None
        };

        Ok((results, next_cursor))
    }

    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, crate::StorageError> {
        let mut results = Vec::new();
        for key in keys {
            results.push(self.backend.get(key).await?);
        }
        Ok(results)
    }

    async fn compare_and_swap(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, crate::StorageError> {
        let current = self.backend.get(key).await?;

        let matches = match (current.as_ref(), expected) {
            (Some(current), Some(expected)) => current.as_slice() == expected,
            (None, None) => true,
            _ => false,
        };

        if matches {
            match new_value {
                Some(value) => self.backend.put(key, value).await?,
                None => self.backend.delete(key).await?,
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
    ) -> Result<i64, crate::StorageError> {
        let current = self.backend.get(key).await?;

        let current_value = match current {
            Some(data) => {
                if data.len() == 8 {
                    i64::from_be_bytes(data.as_slice().try_into().unwrap())
                } else {
                    return Err(crate::StorageError::Backend(
                        "Invalid counter value".to_string(),
                    ));
                }
            }
            None => 0,
        };

        let new_value = current_value + delta;
        let new_bytes = new_value.to_be_bytes();

        self.backend
            .put(key, StorageValue::from(new_bytes.as_slice()))
            .await?;

        Ok(new_value)
    }

    async fn put_with_ttl(
        &self,
        key: &[u8],
        value: StorageValue,
        _ttl: Duration,
    ) -> Result<(), crate::StorageError> {
        // TTL not implemented in basic storage - would need TTL tracking
        self.backend.put(key, value).await
    }

    async fn create_index(
        &self,
        index_name: &str,
        key_extractor: IndexKeyExtractor,
    ) -> Result<(), crate::StorageError> {
        let mut indexes = self.indexes.write().await;
        indexes.insert(index_name.to_string(), key_extractor);
        Ok(())
    }

    async fn query_index(
        &self,
        _index_name: &str,
        _query: IndexQuery,
    ) -> Result<Vec<(StorageValue, StorageValue)>, crate::StorageError> {
        // Index querying not fully implemented - would need index maintenance
        Err(crate::StorageError::InvalidOperation(
            "Index queries not yet implemented".to_string(),
        ))
    }

    async fn export_all(
        &self,
    ) -> Result<Vec<(StorageValue, StorageValue)>, crate::StorageError> {
        self.backend.scan(&[]).await
    }

    async fn import_all(
        &self,
        data: Vec<(StorageValue, StorageValue)>,
    ) -> Result<(), crate::StorageError> {
        for (key, value) in data {
            self.backend.put(key.as_slice(), value).await?;
        }
        Ok(())
    }
}
