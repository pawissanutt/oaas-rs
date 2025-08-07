use crate::{StorageError, StorageValue};
use async_trait::async_trait;
use std::error::Error;
use std::time::Duration;

/// Application data storage - full-featured key-value with transactions
#[async_trait]
pub trait ApplicationDataStorage: crate::StorageBackend {
    type ReadTransaction: ApplicationReadTransaction<Error = StorageError>;
    type WriteTransaction: ApplicationWriteTransaction<Error = StorageError>;

    /// Begin a read-only transaction (potentially more efficient)
    async fn begin_read_transaction(
        &self,
    ) -> Result<Self::ReadTransaction, StorageError>;

    /// Begin a read-write transaction
    async fn begin_write_transaction(
        &self,
    ) -> Result<Self::WriteTransaction, StorageError>;

    /// Range scan with pagination support
    async fn scan_range_paginated(
        &self,
        start: &[u8],
        end: &[u8],
        limit: Option<usize>,
    ) -> Result<
        (Vec<(StorageValue, StorageValue)>, Option<StorageValue>),
        StorageError,
    >;

    /// Multi-get operation for batch reads
    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, StorageError>;

    /// Conditional put operation (compare-and-swap)
    async fn compare_and_swap(
        &self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, StorageError>;

    /// Atomic increment operation
    async fn increment(
        &self,
        key: &[u8],
        delta: i64,
    ) -> Result<i64, StorageError>;

    /// Put with time-to-live support
    async fn put_with_ttl(
        &self,
        key: &[u8],
        value: StorageValue,
        ttl: Duration,
    ) -> Result<(), StorageError>;
}

/// Read-only transaction interface
#[async_trait]
pub trait ApplicationReadTransaction: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    async fn get(
        &self,
        key: &[u8],
    ) -> Result<Option<StorageValue>, Self::Error>;
    async fn multi_get(
        &self,
        keys: Vec<&[u8]>,
    ) -> Result<Vec<Option<StorageValue>>, Self::Error>;
    async fn scan_range(
        &self,
        start: &[u8],
        end: &[u8],
    ) -> Result<Vec<(StorageValue, StorageValue)>, Self::Error>;
    async fn exists(&self, key: &[u8]) -> Result<bool, Self::Error>;
}

/// Read-write transaction interface
#[async_trait]
pub trait ApplicationWriteTransaction: ApplicationReadTransaction {
    async fn put(
        &mut self,
        key: &[u8],
        value: StorageValue,
    ) -> Result<(), Self::Error>;
    async fn delete(&mut self, key: &[u8]) -> Result<(), Self::Error>;
    async fn compare_and_swap(
        &mut self,
        key: &[u8],
        expected: Option<&[u8]>,
        new_value: Option<StorageValue>,
    ) -> Result<bool, Self::Error>;
    async fn commit(self) -> Result<(), Self::Error>;
    async fn rollback(self) -> Result<(), Self::Error>;
}
