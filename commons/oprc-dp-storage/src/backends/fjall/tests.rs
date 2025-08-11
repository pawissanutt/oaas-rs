//! Tests for Fjall storage backend

use super::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SnapshotCapableStorage, StorageBackend, StorageConfig,
        StorageTransaction, StorageValue,
    };
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Helper function to create a temporary Fjall storage instance
    fn create_test_storage() -> (FjallStorage, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let config = StorageConfig::fjall(temp_dir.path().to_string_lossy());
        let storage =
            FjallStorage::new(config).expect("Failed to create FjallStorage");
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_fjall_basic_operations() {
        let (storage, _temp_dir) = create_test_storage();

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
    async fn test_fjall_transactions() {
        let (storage, _temp_dir) = create_test_storage();

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
    async fn test_fjall_scan() {
        let (storage, _temp_dir) = create_test_storage();

        // Insert test data with common prefix
        for i in 0..5 {
            let key = format!("prefix_{:02}", i);
            let value = StorageValue::from(format!("value_{}", i));
            storage.put(key.as_bytes(), value).await.unwrap();
        }

        // Insert data without prefix
        storage
            .put(b"other_key", StorageValue::from("other_value"))
            .await
            .unwrap();

        // Test prefix scan
        let results = storage.scan(b"prefix_").await.unwrap();
        assert_eq!(results.len(), 5);

        // Verify results are in order
        for (i, (key, value)) in results.iter().enumerate() {
            let expected_key = format!("prefix_{:02}", i);
            let expected_value = StorageValue::from(format!("value_{}", i));
            assert_eq!(key.clone().into_vec(), expected_key.into_bytes());
            assert_eq!(value, &expected_value);
        }
    }

    #[tokio::test]
    async fn test_fjall_scan_range() {
        let (storage, _temp_dir) = create_test_storage();

        // Insert test data
        for i in 0..10 {
            let key = format!("key_{:02}", i);
            let value = StorageValue::from(format!("value_{}", i));
            storage.put(key.as_bytes(), value).await.unwrap();
        }

        // Test range scan
        let start = b"key_03".to_vec();
        let end = b"key_07".to_vec();
        let results = storage.scan_range(start..end).await.unwrap();

        // Should include key_03, key_04, key_05, key_06 (end is exclusive)
        assert_eq!(results.len(), 4);

        for (i, (key, value)) in results.iter().enumerate() {
            let expected_key = format!("key_{:02}", i + 3);
            let expected_value = StorageValue::from(format!("value_{}", i + 3));
            assert_eq!(key.clone().into_vec(), expected_key.into_bytes());
            assert_eq!(value, &expected_value);
        }
    }

    #[tokio::test]
    async fn test_fjall_large_values() {
        let (storage, _temp_dir) = create_test_storage();

        let key = b"large_key";
        let large_data = vec![0xAB; 10_000]; // 10KB of data
        let value = StorageValue::from(large_data.clone());

        storage.put(key, value.clone()).await.unwrap();
        let retrieved = storage.get(key).await.unwrap();
        assert_eq!(retrieved, Some(value));

        // Verify the actual data
        let retrieved_data = retrieved.unwrap().into_vec();
        assert_eq!(retrieved_data, large_data);
    }

    #[tokio::test]
    async fn test_fjall_persistence() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().to_string_lossy().to_string();

        let key = b"persistent_key";
        let value = StorageValue::from("persistent_value");

        // First storage instance - write data
        {
            let config = StorageConfig::fjall(&db_path);
            let storage = FjallStorage::new(config).unwrap();
            storage.put(key, value.clone()).await.unwrap();
            // Storage is dropped here
        }

        // Second storage instance - read data
        {
            let config = StorageConfig::fjall(&db_path);
            let storage = FjallStorage::new(config).unwrap();
            let retrieved = storage.get(key).await.unwrap();
            assert_eq!(retrieved, Some(value));
        }
    }

    #[tokio::test]
    async fn test_fjall_delete_range() {
        let (storage, _temp_dir) = create_test_storage();

        // Insert test data
        for i in 0..10 {
            let key = format!("key_{:02}", i);
            let value = StorageValue::from(format!("value_{}", i));
            storage.put(key.as_bytes(), value).await.unwrap();
        }

        // Delete range
        let start = b"key_03".to_vec();
        let end = b"key_07".to_vec();
        let deleted_count = storage.delete_range(start..end).await.unwrap();
        assert_eq!(deleted_count, 4); // key_03, key_04, key_05, key_06

        // Verify deletions
        for i in 3..7 {
            let key = format!("key_{:02}", i);
            assert!(!storage.exists(key.as_bytes()).await.unwrap());
        }

        // Verify other keys still exist
        for i in [0, 1, 2, 7, 8, 9] {
            let key = format!("key_{:02}", i);
            assert!(storage.exists(key.as_bytes()).await.unwrap());
        }
    }

    #[tokio::test]
    async fn test_fjall_put_with_return() {
        let (storage, _temp_dir) = create_test_storage();

        let key = b"test_key";
        let value1 = StorageValue::from("value1");
        let value2 = StorageValue::from("value2");

        // First put should return None (no previous value)
        let previous =
            storage.put_with_return(key, value1.clone()).await.unwrap();
        assert_eq!(previous, None);

        // Second put should return the previous value
        let previous =
            storage.put_with_return(key, value2.clone()).await.unwrap();
        assert_eq!(previous, Some(value1));

        // Verify current value
        let current = storage.get(key).await.unwrap();
        assert_eq!(current, Some(value2));
    }

    #[tokio::test]
    async fn test_fjall_snapshots() {
        let (storage, _temp_dir) = create_test_storage();

        // Add test data
        for i in 0..5 {
            let key = format!("snap_key_{}", i);
            let value = StorageValue::from(format!("snap_value_{}", i));
            storage.put(key.as_bytes(), value).await.unwrap();
        }

        // Create snapshot
        let snapshot = storage.create_snapshot().await.unwrap();

        // Verify snapshot metadata
        assert_eq!(snapshot.entry_count, 5);
        assert!(snapshot.total_size_bytes > 0);
        assert!(!snapshot.snapshot_id.is_empty());

        // Test estimate snapshot size
        let estimated_size = storage
            .estimate_snapshot_size(&snapshot.snapshot_data)
            .await
            .unwrap();
        assert!(estimated_size > 0);

        // Test create KV stream (we need to import Stream)
        use tokio_stream::StreamExt;
        let mut stream =
            storage.create_kv_snapshot_stream(&snapshot).await.unwrap();
        let mut count = 0;
        while let Some(result) = stream.next().await {
            let (key, value) = result.unwrap();
            assert!(key.into_vec().starts_with(b"snap_key_"));
            assert!(value.into_vec().starts_with(b"snap_value_"));
            count += 1;
        }
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn test_fjall_concurrent_operations() {
        let (storage, _temp_dir) = create_test_storage();
        let storage = Arc::new(storage);

        // Spawn multiple tasks to write data concurrently
        let mut handles = Vec::new();

        for i in 0..10 {
            let storage_clone = storage.clone();
            let handle = tokio::spawn(async move {
                let key = format!("concurrent_key_{}", i);
                let value =
                    StorageValue::from(format!("concurrent_value_{}", i));
                storage_clone.put(key.as_bytes(), value).await.unwrap();
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.unwrap();
        }

        // Verify all data was written correctly
        for i in 0..10 {
            let key = format!("concurrent_key_{}", i);
            let expected_value =
                StorageValue::from(format!("concurrent_value_{}", i));
            let actual_value = storage.get(key.as_bytes()).await.unwrap();
            assert_eq!(actual_value, Some(expected_value));
        }
    }

    #[tokio::test]
    async fn test_fjall_empty_operations() {
        let (storage, _temp_dir) = create_test_storage();

        // Test operations on empty storage
        assert_eq!(storage.get(b"nonexistent").await.unwrap(), None);
        assert!(!storage.exists(b"nonexistent").await.unwrap());

        // Test empty prefix scan
        let results = storage.scan(b"nonexistent_prefix").await.unwrap();
        assert!(results.is_empty());

        // Test empty range scan
        let results = storage
            .scan_range(b"a".to_vec()..b"z".to_vec())
            .await
            .unwrap();
        assert!(results.is_empty());

        // Test delete on non-existent key (should not error)
        storage.delete(b"nonexistent").await.unwrap();
    }
}
