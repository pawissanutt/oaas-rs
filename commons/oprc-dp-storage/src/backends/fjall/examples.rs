//! Example usage of the Fjall storage backend
//!
//! This example shows how to use the Fjall backend for persistent storage
//! with transactions, snapshots, and the application storage interface.

#[cfg(feature = "fjall")]
use crate::backends::fjall::FjallStorage;
use crate::traits::application_storage::{
    ApplicationReadTransaction, ApplicationWriteTransaction,
};
use crate::traits::storage_backend::StorageTransaction;
use crate::{
    ApplicationDataStorage, SnapshotCapableStorage, StorageBackend,
    StorageConfig, StorageValue,
};
use std::collections::HashMap;
use std::path::PathBuf;

/// Example demonstrating basic Fjall storage operations
#[cfg(feature = "fjall")]
pub async fn fjall_basic_example() -> Result<(), Box<dyn std::error::Error>> {
    // Create a temporary directory for the database
    let temp_dir = TempDir::new()?;

    // Configure Fjall storage
    let mut config = StorageConfig::default();
    config.path = Some(temp_dir.path().to_path_buf());
    config.cache_size_mb = Some(1); // 1MB cache
                                    // config.write_buffer_size = Some(512 * 1024); // 512KB write buffer - not available in current Fjall version

    // Create the storage backend
    let storage = FjallStorage::new(config)?;

    // Basic key-value operations
    let key = b"example_key";
    let value = StorageValue::from(b"example_value".to_vec());

    // Put a value
    let is_new = storage.put(key, value.clone()).await?;
    println!("Put new key: {}", is_new);

    // Get the value back
    let retrieved = storage.get(key).await?;
    println!("Retrieved value: {:?}", retrieved);

    // Check if key exists
    let exists = storage.exists(key).await?;
    println!("Key exists: {}", exists);

    Ok(())
}

/// Example demonstrating transaction usage
#[cfg(feature = "fjall")]
pub async fn fjall_transaction_example(
) -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let mut config = StorageConfig::default();
    config.path = Some(temp_dir.path().to_path_buf());
    let storage = FjallStorage::new(config)?;

    // Start a transaction
    let mut tx = storage.begin_write_transaction().await?;

    // Perform multiple operations in the transaction
    ApplicationWriteTransaction::put(
        &mut tx,
        b"tx_key1",
        StorageValue::from(b"tx_value1".to_vec()),
    )
    .await?;
    ApplicationWriteTransaction::put(
        &mut tx,
        b"tx_key2",
        StorageValue::from(b"tx_value2".to_vec()),
    )
    .await?;

    // Read within transaction
    let value = ApplicationReadTransaction::get(&tx, b"tx_key1").await?;
    println!("Value in transaction: {:?}", value);

    // Commit the transaction
    ApplicationWriteTransaction::commit(tx).await?;

    // Verify data is persisted
    let persisted_value = storage.get(b"tx_key1").await?;
    println!("Persisted value: {:?}", persisted_value);

    Ok(())
}

/// Example demonstrating snapshot functionality
#[cfg(feature = "fjall")]
pub async fn fjall_snapshot_example() -> Result<(), Box<dyn std::error::Error>>
{
    let temp_dir = TempDir::new()?;
    let mut config = StorageConfig::default();
    config.path = Some(temp_dir.path().to_path_buf());
    let storage = FjallStorage::new(config)?;

    // Add some test data
    for i in 0..10 {
        let key = format!("snap_key_{}", i);
        let value = format!("snap_value_{}", i);
        storage
            .put(key.as_bytes(), StorageValue::from(value.into_bytes()))
            .await?;
    }

    // Create a snapshot
    let snapshot = storage.create_snapshot().await?;
    println!(
        "Created snapshot with {} items",
        snapshot.snapshot_data.len()
    );

    // Estimate snapshot size
    let size = storage
        .estimate_snapshot_size(&snapshot.snapshot_data)
        .await?;
    println!("Estimated snapshot size: {} bytes", size);

    // Create another storage instance to restore into
    let temp_dir2 = TempDir::new()?;
    let mut config2 = StorageConfig::default();
    config2.path = Some(temp_dir2.path().to_path_buf());
    let storage2 = FjallStorage::new(config2)?;

    // Restore from snapshot
    storage2.restore_from_snapshot(&snapshot).await?;

    // Verify data was restored
    let restored_value = storage2.get(b"snap_key_5").await?;
    println!("Restored value: {:?}", restored_value);

    Ok(())
}

/// Example demonstrating advanced features
#[cfg(feature = "fjall")]
pub async fn fjall_advanced_example() -> Result<(), Box<dyn std::error::Error>>
{
    let temp_dir = TempDir::new()?;
    let mut config = StorageConfig::default();
    config.path = Some(temp_dir.path().to_path_buf());
    let storage = FjallStorage::new(config)?;

    // Multi-get operation
    let keys = vec![b"key1", b"key2", b"key3"];

    // First put some values
    for (i, key) in keys.iter().enumerate() {
        let value = format!("value{}", i + 1);
        storage
            .put(&key, StorageValue::from(value.into_bytes()))
            .await?;
    }

    // Multi-get
    let keys: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
    let values = storage.multi_get(keys).await?;
    println!("Multi-get results: {:?}", values);

    // Compare-and-swap operation
    let key = b"cas_key";
    let initial_value = StorageValue::from(b"initial".to_vec());
    let new_value = StorageValue::from(b"updated".to_vec());

    // Put initial value
    storage.put(key, initial_value.clone()).await?;

    // Compare-and-swap with correct expected value
    let success = storage
        .compare_and_swap(key, Some(b"initial"), Some(new_value.clone()))
        .await?;
    println!("CAS success: {}", success);

    // Verify the value was updated
    let final_value = storage.get(key).await?;
    println!("Final value: {:?}", final_value);

    // Atomic increment operation
    let counter_key = b"counter";
    let initial_count = storage.increment(counter_key, 1).await?;
    println!("Initial count: {}", initial_count);

    let incremented_count = storage.increment(counter_key, 5).await?;
    println!("Incremented count: {}", incremented_count);

    // Range scan
    let range_start = b"key1".to_vec();
    let range_end = b"key3".to_vec();
    let range_results = storage.scan_range(range_start..=range_end).await?;
    println!("Range scan results: {} items", range_results.len());

    // Paginated scan
    let (page_results, next_key) = storage
        .scan_range_paginated(b"key1", b"key3", Some(2))
        .await?;
    println!(
        "Paginated results: {} items, next key: {:?}",
        page_results.len(),
        next_key
    );

    Ok(())
}

#[cfg(not(feature = "fjall"))]
pub async fn fjall_not_available() {
    println!(
        "Fjall backend is not available. Enable the 'fjall' feature to use it."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg(feature = "fjall")]
    async fn test_fjall_examples() {
        fjall_basic_example().await.expect("Basic example failed");
        fjall_transaction_example()
            .await
            .expect("Transaction example failed");
        fjall_snapshot_example()
            .await
            .expect("Snapshot example failed");
        fjall_advanced_example()
            .await
            .expect("Advanced example failed");
    }
}
