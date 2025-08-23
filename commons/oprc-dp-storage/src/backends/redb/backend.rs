#![cfg(feature = "redb")]

use async_trait::async_trait;
use std::ops::{Bound, RangeBounds};
use std::path::Path;

use crate::{
    StorageBackend, StorageBackendType, StorageConfig, StorageError,
    StorageResult, StorageStats, StorageValue,
};

use redb::{
    Database, ReadableDatabase, ReadableTable, ReadableTableMetadata,
    TableDefinition,
};

use super::transaction::RedbTransaction;

// Single KV table for this backend
const KV: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("kv");

/// Redb-backed storage
pub struct RedbStorage {
    pub(crate) db: Database,
    pub(crate) config: StorageConfig,
}

impl RedbStorage {
    /// Open or create a Redb database given a `StorageConfig::redb(path)`
    pub fn new(config: StorageConfig) -> StorageResult<Self> {
        if config.backend_type != StorageBackendType::Redb {
            return Err(StorageError::configuration(
                "RedbStorage requires backend_type=Redb",
            ));
        }

        let path = config
            .path
            .clone()
            .ok_or_else(|| StorageError::configuration("Path is required"))?;

        let db = if Path::new(&path).exists() {
            Database::open(path)
                .map_err(|e| StorageError::backend(e.to_string()))?
        } else {
            // Create directory if needed
            if let Some(parent) = Path::new(&path).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .map_err(StorageError::Io)?;
                }
            }
            Database::create(path)
                .map_err(|e| StorageError::backend(e.to_string()))?
        };

        Ok(Self { db, config })
    }
}

#[async_trait]
impl StorageBackend for RedbStorage {
    type Transaction = RedbTransaction;

    async fn begin_transaction(&self) -> StorageResult<Self::Transaction> {
        let wtxn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(RedbTransaction { wtxn: Some(wtxn) })
    }

    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let val = table
            .get(key)
            .map_err(|e| StorageError::backend(e.to_string()))?
            .map(|v| StorageValue::from_slice(v.value()));
        Ok(val)
    }

    async fn put(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<bool> {
        let mut wtxn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let prev_is_some = {
            let mut table = wtxn
                .open_table(KV)
                .map_err(|e| StorageError::backend(e.to_string()))?;
            let prev = table
                .insert(key, value.as_slice())
                .map_err(|e| StorageError::backend(e.to_string()))?;
            prev.is_some()
        };
        wtxn.commit()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(prev_is_some)
    }

    async fn put_with_return(
        &self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<Option<StorageValue>> {
        let mut wtxn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let prev = {
            let mut table = wtxn
                .open_table(KV)
                .map_err(|e| StorageError::backend(e.to_string()))?;
            let prev = table
                .insert(key, value.as_slice())
                .map_err(|e| StorageError::backend(e.to_string()))?;
            prev.map(|v| StorageValue::from_slice(v.value()))
        };
        wtxn.commit()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(prev)
    }

    async fn delete(&self, key: &[u8]) -> StorageResult<()> {
        let mut wtxn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        {
            let mut table = wtxn
                .open_table(KV)
                .map_err(|e| StorageError::backend(e.to_string()))?;
            let _ = table
                .remove(key)
                .map_err(|e| StorageError::backend(e.to_string()))?;
        }
        wtxn.commit()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(())
    }

    async fn delete_range<R>(&self, range: R) -> StorageResult<u64>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        // Prepare concrete bounds and hold the owned buffers locally
        let start_buf: Option<Vec<u8>> = match range.start_bound() {
            Bound::Included(v) => Some(v.clone()),
            Bound::Excluded(v) => Some(v.clone()),
            Bound::Unbounded => None,
        };
        let end_buf: Option<Vec<u8>> = match range.end_bound() {
            Bound::Included(v) => Some(v.clone()),
            Bound::Excluded(v) => Some(v.clone()),
            Bound::Unbounded => None,
        };
        let start_bound: Bound<&[u8]> = match (range.start_bound(), &start_buf)
        {
            (Bound::Included(_), Some(b)) => Bound::Included(b.as_slice()),
            (Bound::Excluded(_), Some(b)) => Bound::Excluded(b.as_slice()),
            _ => Bound::Unbounded,
        };
        let end_bound: Bound<&[u8]> = match (range.end_bound(), &end_buf) {
            (Bound::Included(_), Some(b)) => Bound::Included(b.as_slice()),
            (Bound::Excluded(_), Some(b)) => Bound::Excluded(b.as_slice()),
            _ => Bound::Unbounded,
        };

        let mut wtxn = self
            .db
            .begin_write()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let mut table = wtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;

        // Collect keys to delete first
        let mut keys: Vec<Vec<u8>> = Vec::new();
        for item in table
            .range::<&[u8]>((start_bound, end_bound))
            .map_err(|e| StorageError::backend(e.to_string()))?
        {
            let (k, _v) =
                item.map_err(|e| StorageError::backend(e.to_string()))?;
            keys.push(k.value().to_vec());
        }

        for k in &keys {
            let _ = table
                .remove(k.as_slice())
                .map_err(|e| StorageError::backend(e.to_string()))?;
        }
        // Ensure the table borrow ends before committing the transaction
        drop(table);

        wtxn.commit()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(keys.len() as u64)
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(
            table
                .get(key)
                .map_err(|e| StorageError::backend(e.to_string()))?
                .is_some(),
        )
    }

    async fn scan(
        &self,
        prefix: &[u8],
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>> {
        // Determine the exclusive end bound for the prefix
        let mut end_vec: Option<Vec<u8>> = None;
        let end_bound: Bound<&[u8]> = if prefix.is_empty() {
            Bound::Unbounded
        } else {
            let mut v = prefix.to_vec();
            if let Some(last) = v.last_mut() {
                if *last == u8::MAX {
                    v.push(0);
                } else {
                    *last += 1;
                }
            }
            end_vec = Some(v);
            Bound::Excluded(end_vec.as_ref().unwrap().as_slice())
        };

        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;

        let mut out = Vec::new();
        for item in table
            .range::<&[u8]>((Bound::Included(prefix), end_bound))
            .map_err(|e| StorageError::backend(e.to_string()))?
        {
            let (k, v) =
                item.map_err(|e| StorageError::backend(e.to_string()))?;
            out.push((
                StorageValue::from_slice(k.value()),
                StorageValue::from_slice(v.value()),
            ));
        }
        drop(table);
        Ok(out)
    }

    async fn scan_range<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;

        // Prepare bounds
        let start_buf: Option<Vec<u8>> = match range.start_bound() {
            Bound::Included(v) => Some(v.clone()),
            Bound::Excluded(v) => Some(v.clone()),
            Bound::Unbounded => None,
        };
        let end_buf: Option<Vec<u8>> = match range.end_bound() {
            Bound::Included(v) => Some(v.clone()),
            Bound::Excluded(v) => Some(v.clone()),
            Bound::Unbounded => None,
        };
        let start_bound: Bound<&[u8]> = match (range.start_bound(), &start_buf)
        {
            (Bound::Included(_), Some(b)) => Bound::Included(b.as_slice()),
            (Bound::Excluded(_), Some(b)) => Bound::Excluded(b.as_slice()),
            _ => Bound::Unbounded,
        };
        let end_bound: Bound<&[u8]> = match (range.end_bound(), &end_buf) {
            (Bound::Included(_), Some(b)) => Bound::Included(b.as_slice()),
            (Bound::Excluded(_), Some(b)) => Bound::Excluded(b.as_slice()),
            _ => Bound::Unbounded,
        };
        let mut out = Vec::new();
        for item in table
            .range::<&[u8]>((start_bound, end_bound))
            .map_err(|e| StorageError::backend(e.to_string()))?
        {
            let (k, v) =
                item.map_err(|e| StorageError::backend(e.to_string()))?;
            out.push((
                StorageValue::from_slice(k.value()),
                StorageValue::from_slice(v.value()),
            ));
        }
        Ok(out)
    }

    async fn scan_range_reverse<R>(
        &self,
        range: R,
    ) -> StorageResult<Vec<(StorageValue, StorageValue)>>
    where
        R: RangeBounds<Vec<u8>> + Send,
    {
        let mut items = self.scan_range(range).await?;
        items.reverse();
        Ok(items)
    }

    async fn get_last(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let res = table
            .last()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        match res {
            Some((k, v)) => Ok(Some((
                StorageValue::from_slice(k.value()),
                StorageValue::from_slice(v.value()),
            ))),
            None => Ok(None),
        }
    }

    async fn get_first(
        &self,
    ) -> StorageResult<Option<(StorageValue, StorageValue)>> {
        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let res = table
            .first()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        match res {
            Some((k, v)) => Ok(Some((
                StorageValue::from_slice(k.value()),
                StorageValue::from_slice(v.value()),
            ))),
            None => Ok(None),
        }
    }

    async fn count(&self) -> StorageResult<u64> {
        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let len = table
            .len()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(len)
    }

    async fn flush(&self) -> StorageResult<()> {
        // redb is transactional; an explicit flush isn't necessary for read-only DB
        Ok(())
    }

    async fn close(&self) -> StorageResult<()> {
        // Dropping Database closes it
        // Let the caller drop the storage; nothing to do here
        Ok(())
    }

    fn backend_type(&self) -> StorageBackendType {
        StorageBackendType::Redb
    }

    async fn stats(&self) -> StorageResult<StorageStats> {
        use std::collections::HashMap;

        let rtxn = self
            .db
            .begin_read()
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let table = rtxn
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let mut entries = 0u64;
        let mut total = 0u64;
        for item in table
            .iter()
            .map_err(|e| StorageError::backend(e.to_string()))?
        {
            let (k, v) =
                item.map_err(|e| StorageError::backend(e.to_string()))?;
            entries += 1;
            total += (k.value().len() + v.value().len()) as u64;
        }
        Ok(StorageStats {
            entries_count: entries,
            total_size_bytes: total,
            memory_usage_bytes: None,
            disk_usage_bytes: None,
            cache_hit_rate: None,
            backend_specific: HashMap::new(),
        })
    }

    async fn compact(&self) -> StorageResult<()> {
        // redb compaction is managed internally; no-op for now
        Ok(())
    }
}
