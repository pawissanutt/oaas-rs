#![cfg(feature = "redb")]

use async_trait::async_trait;

use crate::{StorageError, StorageResult, StorageTransaction, StorageValue};

use redb::{ReadableTable, TableDefinition};

// Single KV table for this backend
const KV: TableDefinition<'static, &'static [u8], &'static [u8]> =
    TableDefinition::new("kv");

/// Redb-backed transaction (write transaction) — Option 1: reopen table per call
pub struct RedbTransaction {
    pub(crate) wtxn: Option<redb::WriteTransaction>,
}

#[async_trait(?Send)]
impl StorageTransaction for RedbTransaction {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>> {
        let table = self
            .wtxn
            .as_ref()
            .ok_or_else(|| StorageError::transaction("transaction closed"))?
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let val = table
            .get(key)
            .map_err(|e| StorageError::backend(e.to_string()))?
            .map(|v| StorageValue::from_slice(v.value()));
        Ok(val)
    }

    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> StorageResult<()> {
        let mut table = self
            .wtxn
            .as_ref()
            .ok_or_else(|| StorageError::transaction("transaction closed"))?
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        table
            .insert(key, value.as_slice())
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(())
    }

    async fn delete(&mut self, key: &[u8]) -> StorageResult<()> {
        let mut table = self
            .wtxn
            .as_ref()
            .ok_or_else(|| StorageError::transaction("transaction closed"))?
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let _ = table
            .remove(key)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(())
    }

    async fn exists(&self, key: &[u8]) -> StorageResult<bool> {
        let table = self
            .wtxn
            .as_ref()
            .ok_or_else(|| StorageError::transaction("transaction closed"))?
            .open_table(KV)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        let got = table
            .get(key)
            .map_err(|e| StorageError::backend(e.to_string()))?;
        Ok(got.is_some())
    }

    async fn commit(self) -> StorageResult<()> {
        if let Some(wtxn) = self.wtxn {
            wtxn.commit()
                .map_err(|e| StorageError::backend(e.to_string()))?;
        }
        Ok(())
    }

    async fn rollback(self) -> StorageResult<()> {
        // Dropping the write transaction will abort it
        Ok(())
    }
}
