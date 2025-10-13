use async_trait::async_trait;
use std::ops::RangeBounds;
use std::sync::Arc;
use tokio_stream::Stream;

#[cfg(feature = "fjall")]
use crate::backends::fjall::FjallStorage;
use crate::backends::memory::MemoryStorage;
#[cfg(feature = "skiplist")]
use crate::backends::skiplist::SkipListStorage;
use crate::snapshot::{Snapshot, SnapshotCapableStorage};
use crate::traits as storage_traits;
use crate::traits::{
    ApplicationDataStorage, ApplicationReadTransaction,
    ApplicationWriteTransaction, StorageBackend, StorageTransaction,
};
use crate::{
    StorageBackendType, StorageConfig, StorageError, StorageResult,
    StorageStats, StorageValue,
};

#[derive(Clone)]
pub enum AnyStorage {
    Memory(MemoryStorage),
    #[cfg(feature = "skiplist")]
    SkipList(SkipListStorage),
    #[cfg(feature = "fjall")]
    Fjall(FjallStorage),
}

impl AnyStorage {
    pub fn open(config: StorageConfig) -> StorageResult<Self> {
        match config.backend_type {
            // Treat Memory backend_type as SkipList when feature enabled, else Memory
            StorageBackendType::Memory => {
                #[cfg(feature = "skiplist")]
                {
                    return Ok(AnyStorage::SkipList(SkipListStorage::new(
                        config,
                    )?));
                }
                #[cfg(not(feature = "skiplist"))]
                {
                    return Ok(AnyStorage::Memory(MemoryStorage::new(config)?));
                }
            }
            StorageBackendType::SkipList => {
                #[cfg(feature = "skiplist")]
                {
                    return Ok(AnyStorage::SkipList(SkipListStorage::new(
                        config,
                    )?));
                }
                #[cfg(not(feature = "skiplist"))]
                {
                    return Err(StorageError::configuration(
                        "SkipList backend feature not enabled",
                    ));
                }
            }
            StorageBackendType::Fjall => {
                #[cfg(feature = "fjall")]
                {
                    return Ok(AnyStorage::Fjall(FjallStorage::new(config)?));
                }
                #[cfg(not(feature = "fjall"))]
                {
                    return Err(StorageError::configuration(
                        "Fjall backend feature not enabled",
                    ));
                }
            }
            // Additional backends can be added here in the future
            _ => Err(StorageError::configuration(
                "Only memory/skiplist backends supported in PoC",
            )),
        }
    }
}

pub enum AnyTransaction {
    Memory(<MemoryStorage as StorageBackend>::Transaction<'static>),
    #[cfg(feature = "skiplist")]
    SkipList(<SkipListStorage as StorageBackend>::Transaction<'static>),
}

// Helper to wrap lifetimes for transactions by boxing them; implement as concrete enums with 'static using unsafe? Instead avoid exposing begin_transaction that requires 'static. We will use wrapper that holds concrete inner types with erased lifetime via Box.
pub enum AnyBorrowedTransaction<'a> {
    Memory(<MemoryStorage as StorageBackend>::Transaction<'a>),
    #[cfg(feature = "skiplist")]
    SkipList(<SkipListStorage as StorageBackend>::Transaction<'a>),
    #[cfg(feature = "fjall")]
    Fjall(<FjallStorage as StorageBackend>::Transaction<'a>),
}

#[async_trait(?Send)]
impl<'a> StorageTransaction for AnyBorrowedTransaction<'a> {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        match self {
            Self::Memory(t) => {
                storage_traits::StorageTransaction::get(t, key).await
            }
            #[cfg(feature = "skiplist")]
            Self::SkipList(t) => {
                storage_traits::StorageTransaction::get(t, key).await
            }
            #[cfg(feature = "fjall")]
            Self::Fjall(t) => {
                storage_traits::StorageTransaction::get(t, key).await
            }
        }
    }

    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()> {
        match self {
            Self::Memory(t) => {
                storage_traits::StorageTransaction::put(t, key, value).await
            }
            #[cfg(feature = "skiplist")]
            Self::SkipList(t) => {
                storage_traits::StorageTransaction::put(t, key, value).await
            }
            #[cfg(feature = "fjall")]
            Self::Fjall(t) => {
                storage_traits::StorageTransaction::put(t, key, value).await
            }
        }
    }

    async fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        match self {
            Self::Memory(t) => {
                storage_traits::StorageTransaction::delete(t, key).await
            }
            #[cfg(feature = "skiplist")]
            Self::SkipList(t) => {
                storage_traits::StorageTransaction::delete(t, key).await
            }
            #[cfg(feature = "fjall")]
            Self::Fjall(t) => {
                storage_traits::StorageTransaction::delete(t, key).await
            }
        }
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        match self {
            Self::Memory(t) => {
                storage_traits::StorageTransaction::exists(t, key).await
            }
            #[cfg(feature = "skiplist")]
            Self::SkipList(t) => {
                storage_traits::StorageTransaction::exists(t, key).await
            }
            #[cfg(feature = "fjall")]
            Self::Fjall(t) => {
                storage_traits::StorageTransaction::exists(t, key).await
            }
        }
    }

    async fn commit(self) -> StorageResult<()> {
        match self {
            Self::Memory(t) => {
                storage_traits::StorageTransaction::commit(t).await
            }
            #[cfg(feature = "skiplist")]
            Self::SkipList(t) => {
                storage_traits::StorageTransaction::commit(t).await
            }
            #[cfg(feature = "fjall")]
            Self::Fjall(t) => {
                storage_traits::StorageTransaction::commit(t).await
            }
        }
    }

    async fn rollback(self) -> StorageResult<()> {
        match self {
            Self::Memory(t) => {
                storage_traits::StorageTransaction::rollback(t).await
            }
            #[cfg(feature = "skiplist")]
            Self::SkipList(t) => {
                storage_traits::StorageTransaction::rollback(t).await
            }
            #[cfg(feature = "fjall")]
            Self::Fjall(t) => {
                storage_traits::StorageTransaction::rollback(t).await
            }
        }
    }
}

#[async_trait]
impl StorageBackend for AnyStorage {
    type Transaction<'a>
        = AnyBorrowedTransaction<'a>
    where
        Self: 'a;

    fn begin_transaction(&self) -> StorageResult<Self::Transaction<'_>> {
        match self {
            AnyStorage::Memory(s) => {
                Ok(AnyBorrowedTransaction::Memory(s.begin_transaction()?))
            }
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => {
                Ok(AnyBorrowedTransaction::SkipList(s.begin_transaction()?))
            }
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => {
                Ok(AnyBorrowedTransaction::Fjall(s.begin_transaction()?))
            }
        }
    }

    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        match self {
            AnyStorage::Memory(s) => s.get(key).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.get(key).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.get(key).await,
        }
    }

    async fn put(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<bool> {
        match self {
            AnyStorage::Memory(s) => s.put(key, value).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.put(key, value).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.put(key, value).await,
        }
    }

    async fn put_with_return(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<Option<StorageValue>> {
        match self {
            AnyStorage::Memory(s) => s.put_with_return(key, value).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.put_with_return(key, value).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.put_with_return(key, value).await,
        }
    }

    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        match self {
            AnyStorage::Memory(s) => s.delete(key).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.delete(key).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.delete(key).await,
        }
    }

    async fn delete_range<R>(&self, range: R) -> StorageResult<u64>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        match self {
            AnyStorage::Memory(s) => s.delete_range(range).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.delete_range(range).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.delete_range(range).await,
        }
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        match self {
            AnyStorage::Memory(s) => s.exists(key).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.exists(key).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.exists(key).await,
        }
    }

    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        match self {
            AnyStorage::Memory(s) => s.scan(prefix).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.scan(prefix).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.scan(prefix).await,
        }
    }

    async fn scan_range<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        match self {
            AnyStorage::Memory(s) => s.scan_range(range).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.scan_range(range).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.scan_range(range).await,
        }
    }

    async fn scan_range_reverse<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        match self {
            AnyStorage::Memory(s) => s.scan_range_reverse(range).await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.scan_range_reverse(range).await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.scan_range_reverse(range).await,
        }
    }

    async fn get_last(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        match self {
            AnyStorage::Memory(s) => s.get_last().await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.get_last().await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.get_last().await,
        }
    }

    async fn get_first(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        match self {
            AnyStorage::Memory(s) => s.get_first().await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.get_first().await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.get_first().await,
        }
    }

    async fn count(&self) -> StorageResult<u64> {
        match self {
            AnyStorage::Memory(s) => s.count().await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.count().await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.count().await,
        }
    }

    async fn flush(&self) -> StorageResult<()> {
        match self {
            AnyStorage::Memory(s) => s.flush().await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.flush().await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.flush().await,
        }
    }

    async fn compact(&self) -> StorageResult<()> {
        match self {
            AnyStorage::Memory(s) => s.compact().await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.compact().await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.compact().await,
        }
    }

    async fn close(&self) -> StorageResult<()> {
        match self {
            AnyStorage::Memory(s) => s.close().await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.close().await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.close().await,
        }
    }

    fn backend_type(&self) -> StorageBackendType {
        match self {
            AnyStorage::Memory(s) => s.backend_type(),
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.backend_type(),
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.backend_type(),
        }
    }

    async fn stats(&self) -> StorageResult<StorageStats> {
        match self {
            AnyStorage::Memory(s) => s.stats().await,
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(s) => s.stats().await,
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => s.stats().await,
        }
    }
}

// Backend-aware snapshot data: either materialized KV pairs or native Fjall snapshot
#[derive(Clone)]
pub enum AnySnapshotData {
    KvPairs(Arc<Vec<(StorageValue, StorageValue)>>),
    #[cfg(feature = "fjall")]
    Fjall(Arc<fjall::Snapshot>),
}

struct AnySnapshotStream {
    items:
        std::vec::IntoIter<Result<(StorageValue, StorageValue), StorageError>>,
}

impl AnySnapshotStream {
    fn from_data(data: &AnySnapshotData) -> Self {
        let items: Vec<Result<(StorageValue, StorageValue), StorageError>> =
            match data {
                AnySnapshotData::KvPairs(v) => {
                    v.iter().cloned().map(Ok).collect()
                }
                #[cfg(feature = "fjall")]
                AnySnapshotData::Fjall(snap) => snap
                    .iter()
                    .map(|res| {
                        res.map(|(k, v)| {
                            (
                                StorageValue::from(k.to_vec()),
                                StorageValue::from(v.to_vec()),
                            )
                        })
                        .map_err(|e| {
                            StorageError::backend(format!(
                                "Fjall snapshot iter error: {}",
                                e
                            ))
                        })
                    })
                    .collect(),
            };
        Self {
            items: items.into_iter(),
        }
    }
}

impl Stream for AnySnapshotStream {
    type Item = Result<(StorageValue, StorageValue), StorageError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        match self.items.next() {
            Some(item) => std::task::Poll::Ready(Some(item)),
            None => std::task::Poll::Ready(None),
        }
    }
}

#[async_trait]
impl SnapshotCapableStorage for AnyStorage {
    type SnapshotData = AnySnapshotData;

    async fn create_snapshot(
        &self,
    ) -> Result<Snapshot<Self::SnapshotData>, StorageError> {
        match self {
            AnyStorage::Memory(_) => {
                let range: (
                    std::ops::Bound<Vec<u8>>,
                    std::ops::Bound<Vec<u8>>,
                ) = (std::ops::Bound::Unbounded, std::ops::Bound::Unbounded);
                let all = self.scan_range(range).await?;
                let entry_count = all.len() as u64;
                let total_size_bytes =
                    all.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
                        as u64;
                Ok(Snapshot {
                    snapshot_id: format!(
                        "any_snapshot_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    ),
                    created_at: std::time::SystemTime::now(),
                    sequence_number: entry_count,
                    snapshot_data: AnySnapshotData::KvPairs(Arc::new(all)),
                    entry_count,
                    total_size_bytes,
                    compression: crate::CompressionType::None,
                })
            }
            #[cfg(feature = "skiplist")]
            AnyStorage::SkipList(_) => {
                let range: (
                    std::ops::Bound<Vec<u8>>,
                    std::ops::Bound<Vec<u8>>,
                ) = (std::ops::Bound::Unbounded, std::ops::Bound::Unbounded);
                let all = self.scan_range(range).await?;
                let entry_count = all.len() as u64;
                let total_size_bytes =
                    all.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
                        as u64;
                Ok(Snapshot {
                    snapshot_id: format!(
                        "any_snapshot_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    ),
                    created_at: std::time::SystemTime::now(),
                    sequence_number: entry_count,
                    snapshot_data: AnySnapshotData::KvPairs(Arc::new(all)),
                    entry_count,
                    total_size_bytes,
                    compression: crate::CompressionType::None,
                })
            }
            #[cfg(feature = "fjall")]
            AnyStorage::Fjall(s) => {
                let fjall_snapshot = s.partition().snapshot();
                let snapshot_data = Arc::new(fjall_snapshot);
                let entry_count = snapshot_data.len().map_err(|e| {
                    StorageError::backend(format!(
                        "Fjall snapshot len error: {}",
                        e
                    ))
                })? as u64;

                let mut estimated_size = 0u64;
                if let Some((k, v)) =
                    snapshot_data.first_key_value().map_err(|e| {
                        StorageError::backend(format!(
                            "Fjall snapshot first_kv error: {}",
                            e
                        ))
                    })?
                {
                    estimated_size += k.len() as u64 + v.len() as u64;
                }
                if let Some((k, v)) =
                    snapshot_data.last_key_value().map_err(|e| {
                        StorageError::backend(format!(
                            "Fjall snapshot last_kv error: {}",
                            e
                        ))
                    })?
                {
                    estimated_size += k.len() as u64 + v.len() as u64;
                }
                let total_size_bytes = if estimated_size > 0 {
                    (estimated_size / 2) * entry_count
                } else {
                    0
                };

                Ok(Snapshot {
                    snapshot_id: format!(
                        "any_fjall_snapshot_{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_nanos()
                    ),
                    created_at: std::time::SystemTime::now(),
                    sequence_number: 0,
                    snapshot_data: AnySnapshotData::Fjall(snapshot_data),
                    entry_count,
                    total_size_bytes,
                    compression: crate::CompressionType::None,
                })
            }
        }
    }

    async fn restore_from_snapshot(
        &self,
        snapshot: &Snapshot<Self::SnapshotData>,
    ) -> Result<(), StorageError> {
        // Delegate to installation via stream for consistency
        let stream = self.create_kv_snapshot_stream(snapshot).await?;
        self.install_kv_snapshot_from_stream(stream).await
    }

    async fn latest_snapshot(
        &self,
    ) -> Result<Option<Snapshot<Self::SnapshotData>>, StorageError> {
        Ok(Some(self.create_snapshot().await?))
    }

    async fn estimate_snapshot_size(
        &self,
        snapshot_data: &Self::SnapshotData,
    ) -> Result<u64, StorageError> {
        match snapshot_data {
            AnySnapshotData::KvPairs(v) => {
                Ok(v.iter().map(|(k, v)| (k.len() + v.len()) as u64).sum())
            }
            #[cfg(feature = "fjall")]
            AnySnapshotData::Fjall(snap) => {
                let mut size = 0u64;
                for res in snap.iter() {
                    let (k, v) = res.map_err(|e| {
                        StorageError::backend(format!(
                            "Fjall snapshot iter error: {}",
                            e
                        ))
                    })?;
                    size += k.len() as u64 + v.len() as u64;
                }
                Ok(size)
            }
        }
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
        Ok(Box::new(AnySnapshotStream::from_data(
            &snapshot.snapshot_data,
        )))
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

        let range: (std::ops::Bound<Vec<u8>>, std::ops::Bound<Vec<u8>>) =
            (std::ops::Bound::Unbounded, std::ops::Bound::Unbounded);
        let _ = self.delete_range(range).await?;

        while let Some(item) = stream.next().await {
            let (k, v) = item?;
            let _ = self.put(k.as_slice(), v).await?;
        }
        Ok(())
    }
}

pub struct AnyAppTransaction<'a> {
    inner: AnyBorrowedTransaction<'a>,
}

#[async_trait(?Send)]
impl<'a> ApplicationReadTransaction for AnyAppTransaction<'a> {
    type Error = StorageError;

    async fn get(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, Self::Error> {
        StorageTransaction::get(&self.inner, key).await
    }

    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, Self::Error> {
        let mut results = Vec::with_capacity(keys.len());
        for k in keys {
            results.push(StorageTransaction::get(&self.inner, k).await?);
        }
        Ok(results)
    }

    async fn scan_range(
        &self,
        _start: &[u8],
        _end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error> {
        // Fallback: not efficient, but use backend scan_range via owner is not available here
        // This method is only used in memory implementation; for AnyStorage we won't use it in tx
        Err(StorageError::backend(
            "Transaction scan_range not supported in AnyStorage",
        ))
    }

    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error> {
        StorageTransaction::exists(&self.inner, key).await
    }
}

#[async_trait(?Send)]
impl<'a> ApplicationWriteTransaction for AnyAppTransaction<'a> {
    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> Result<(), Self::Error> {
        StorageTransaction::put(&mut self.inner, key, value).await
    }

    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error> {
        StorageTransaction::delete(&mut self.inner, key).await
    }

    async fn compare_and_swap(
        &mut self,
        _key: &[u8],
        _expected: Option<&[u8]>,
        _new_value: Option<StorageValue>,
    ) -> Result<bool, Self::Error> {
        // Not supported at transaction level in generic impl
        Err(StorageError::backend(
            "Transaction CAS not supported in AnyStorage",
        ))
    }

    async fn commit(self) -> Result<(), Self::Error> {
        StorageTransaction::commit(self.inner).await
    }

    async fn rollback(self) -> Result<(), Self::Error> {
        StorageTransaction::rollback(self.inner).await
    }
}

#[async_trait]
impl ApplicationDataStorage for AnyStorage {
    type ReadTransaction<'a>
        = AnyAppTransaction<'a>
    where
        Self: 'a;
    type WriteTransaction<'a>
        = AnyAppTransaction<'a>
    where
        Self: 'a;

    fn begin_read_transaction(
        &self,
    ) -> Result<Self::ReadTransaction<'_>, StorageError> {
        Ok(AnyAppTransaction {
            inner: self.begin_transaction()?,
        })
    }

    fn begin_write_transaction(
        &self,
    ) -> Result<Self::WriteTransaction<'_>, StorageError> {
        Ok(AnyAppTransaction {
            inner: self.begin_transaction()?,
        })
    }

    async fn scan_range_paginated(
        &self,
        _start: &[u8],
        _end: &[u8],
        limit: Option<usize>,
    ) -> Result<
        (Vec<(StorageValue, StorageValue)>, Option<StorageValue>),
        StorageError,
    > {
        let range = _start.to_vec().._end.to_vec();
        let mut results = StorageBackend::scan_range(self, range).await?;

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
        // Generic, not strictly atomic across variants; acceptable for PoC and SkipList
        let current = self.get(key).await?;
        let current_matches = match (current.as_ref(), expected) {
            (Some(current), Some(expected)) => current.as_slice() == expected,
            (None, None) => true,
            _ => false,
        };

        if current_matches {
            match new_value {
                Some(v) => {
                    let _ = self.put(key, v).await?;
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
        let current = match self.get(key).await? {
            Some(value) => {
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
            None => 0,
        };

        let new_value = current + delta;
        let value_bytes = new_value.to_le_bytes();
        let new_storage_value = StorageValue::from(value_bytes.as_slice());
        let _ = self.put(key, new_storage_value).await?;
        Ok(new_value)
    }

    async fn put_with_ttl(
        &self,
        key: &[u8],
        value: StorageValue,
        _ttl: std::time::Duration,
    ) -> Result<bool, StorageError> {
        self.put(key, value).await
    }
}
