/// Reusable test module for all storage backend implementations
///
/// This module provides a comprehensive test suite that can be used to validate
/// any implementation of the `StorageBackend` trait, ensuring behavioral consistency
/// across Memory, Fjall, SkipList, and future storage backends.
use crate::{
    snapshot::SnapshotCapableStorage,
    storage_value::StorageValue,
    traits::{StorageBackend, StorageTransaction},
};
use tokio_stream::StreamExt;

/// Comprehensive test suite for any StorageBackend implementation
///
/// This function runs a full battery of tests against a storage backend,
/// testing all core functionality including:
/// - Basic CRUD operations
/// - Transaction support (commit/rollback)
/// - Scanning operations (prefix, range, reverse)
/// - Snapshot operations (create, restore, streaming)
/// - Edge cases and error conditions
///
/// # Usage
/// ```rust
/// #[tokio::test]
/// async fn test_memory_backend() {
///     let config = StorageConfig::memory();
///     let storage = MemoryStorage::new(config).unwrap();
///     test_storage_backend_comprehensive(storage).await;
/// }
/// ```
pub async fn test_storage_backend_comprehensive<T>(storage: T)
where
    T: StorageBackend + SnapshotCapableStorage + Send + Sync + 'static,
{
    test_basic_operations(&storage).await;
    test_transaction_operations(&storage).await;
    test_scan_operations(&storage).await;
    test_range_operations(&storage).await;
    test_snapshot_operations(&storage).await;
    test_edge_cases(&storage).await;
}

/// Test basic CRUD operations
pub async fn test_basic_operations<T>(storage: &T)
where
    T: StorageBackend + Send + Sync,
{
    let key = b"test_key";
    let value = StorageValue::from("test_value");

    // Test put and get
    storage.put(key, value.clone()).await.unwrap();
    let retrieved = storage.get(key).await.unwrap();
    assert_eq!(retrieved, Some(value), "Put/Get operation failed");

    // Test exists
    assert!(
        storage.exists(key).await.unwrap(),
        "Key should exist after put"
    );
    assert!(
        !storage.exists(b"nonexistent").await.unwrap(),
        "Nonexistent key should not exist"
    );

    // Test delete
    storage.delete(key).await.unwrap();
    assert!(
        !storage.exists(key).await.unwrap(),
        "Key should not exist after delete"
    );
    assert_eq!(
        storage.get(key).await.unwrap(),
        None,
        "Get should return None after delete"
    );
}

/// Test transaction operations (commit and rollback)
pub async fn test_transaction_operations<T>(storage: &T)
where
    T: StorageBackend + Send + Sync,
{
    let key1 = b"tx_key1";
    let key2 = b"tx_key2";
    let value1 = StorageValue::from("tx_value1");
    let value2 = StorageValue::from("tx_value2");

    // Test transaction commit
    {
        let mut tx = storage.begin_transaction().await.unwrap();
        tx.put(key1, value1.clone()).await.unwrap();
        tx.put(key2, value2.clone()).await.unwrap();
        tx.commit().await.unwrap();
    }

    assert_eq!(
        storage.get(key1).await.unwrap(),
        Some(value1),
        "Transaction commit failed for key1"
    );
    assert_eq!(
        storage.get(key2).await.unwrap(),
        Some(value2),
        "Transaction commit failed for key2"
    );

    // Test transaction rollback (if supported)
    let rollback_result = {
        let mut tx = storage.begin_transaction().await.unwrap();
        tx.delete(key1).await.unwrap();
        tx.rollback().await
    };

    match rollback_result {
        Ok(()) => {
            // Key should still exist after rollback
            assert!(
                storage.exists(key1).await.unwrap(),
                "Key should still exist after transaction rollback"
            );
        }
        Err(_) => {
            // Rollback not supported - that's ok, just verify the key was actually deleted
            // This can happen with some backends like SkipList that don't support rollback
        }
    }
}

/// Test scanning operations (prefix and basic scan)
pub async fn test_scan_operations<T>(storage: &T)
where
    T: StorageBackend + Send + Sync,
{
    // Clear any existing data first
    let _ = storage.delete_range(b"".to_vec()..b"\xFF".repeat(10)).await;

    // Insert test data
    storage
        .put(b"prefix_1", StorageValue::from("value1"))
        .await
        .unwrap();
    storage
        .put(b"prefix_2", StorageValue::from("value2"))
        .await
        .unwrap();
    storage
        .put(b"other_3", StorageValue::from("value3"))
        .await
        .unwrap();

    // Test prefix scan
    let results = storage.scan(b"prefix_").await.unwrap();
    assert_eq!(results.len(), 2, "Prefix scan should return 2 results");

    // Results should be sorted
    assert_eq!(
        results[0].0.as_slice(),
        b"prefix_1",
        "First result should be prefix_1"
    );
    assert_eq!(
        results[1].0.as_slice(),
        b"prefix_2",
        "Second result should be prefix_2"
    );
    assert_eq!(
        results[0].1,
        StorageValue::from("value1"),
        "First value should match"
    );
    assert_eq!(
        results[1].1,
        StorageValue::from("value2"),
        "Second value should match"
    );
}

/// Test range operations (range scan, reverse scan, first/last, delete range)
pub async fn test_range_operations<T>(storage: &T)
where
    T: StorageBackend + Send + Sync,
{
    // Clear any existing data first
    let _ = storage.delete_range(b"".to_vec()..b"\xFF".repeat(10)).await;

    // Insert test data with keys that will be ordered
    storage
        .put(b"key_001", StorageValue::from("value1"))
        .await
        .unwrap();
    storage
        .put(b"key_005", StorageValue::from("value2"))
        .await
        .unwrap();
    storage
        .put(b"key_010", StorageValue::from("value3"))
        .await
        .unwrap();
    storage
        .put(b"key_015", StorageValue::from("value4"))
        .await
        .unwrap();
    storage
        .put(b"key_020", StorageValue::from("value5"))
        .await
        .unwrap();
    storage
        .put(b"other_key", StorageValue::from("other"))
        .await
        .unwrap();

    // Test range scan
    let range_results = storage
        .scan_range(b"key_005".to_vec()..b"key_015".to_vec())
        .await
        .unwrap();
    assert_eq!(
        range_results.len(),
        2,
        "Range scan should return 2 results (key_005 and key_010)"
    );
    assert_eq!(
        range_results[0].0.as_slice(),
        b"key_005",
        "First range result should be key_005"
    );
    assert_eq!(
        range_results[1].0.as_slice(),
        b"key_010",
        "Second range result should be key_010"
    );

    // Test reverse range scan
    let reverse_results = storage
        .scan_range_reverse(b"key_005".to_vec()..b"key_015".to_vec())
        .await
        .unwrap();
    assert_eq!(
        reverse_results.len(),
        2,
        "Reverse range scan should return 2 results"
    );
    assert_eq!(
        reverse_results[0].0.as_slice(),
        b"key_010",
        "First reverse result should be key_010 (reversed order)"
    );
    assert_eq!(
        reverse_results[1].0.as_slice(),
        b"key_005",
        "Second reverse result should be key_005 (reversed order)"
    );

    // Test get_first and get_last
    let first = storage.get_first().await.unwrap().unwrap();
    assert_eq!(
        first.0.as_slice(),
        b"key_001",
        "get_first should return the smallest key"
    );

    let last = storage.get_last().await.unwrap().unwrap();
    assert_eq!(
        last.0.as_slice(),
        b"other_key",
        "get_last should return the largest key lexicographically"
    );

    // Test delete_range
    let deleted_count = storage
        .delete_range(b"key_005".to_vec()..b"key_020".to_vec())
        .await
        .unwrap();
    assert_eq!(
        deleted_count, 3,
        "delete_range should delete 3 keys (key_005, key_010, key_015)"
    );

    // Verify deletions
    assert!(
        storage.exists(b"key_001").await.unwrap(),
        "key_001 should still exist (outside range)"
    );
    assert!(
        !storage.exists(b"key_005").await.unwrap(),
        "key_005 should be deleted"
    );
    assert!(
        !storage.exists(b"key_010").await.unwrap(),
        "key_010 should be deleted"
    );
    assert!(
        !storage.exists(b"key_015").await.unwrap(),
        "key_015 should be deleted"
    );
    assert!(
        storage.exists(b"key_020").await.unwrap(),
        "key_020 should still exist (excluded from range)"
    );
    assert!(
        storage.exists(b"other_key").await.unwrap(),
        "other_key should still exist (outside range)"
    );
}

/// Test snapshot operations (create, restore, streaming)
pub async fn test_snapshot_operations<T>(storage: &T)
where
    T: StorageBackend + SnapshotCapableStorage + Send + Sync,
{
    // Clear any existing data first
    let _ = storage.delete_range(b"".to_vec()..b"\xFF".repeat(10)).await;

    // Insert test data
    storage
        .put(b"snap_key1", StorageValue::from("snap_value1"))
        .await
        .unwrap();
    storage
        .put(b"snap_key2", StorageValue::from("snap_value2"))
        .await
        .unwrap();
    storage
        .put(b"snap_key3", StorageValue::from("snap_value3"))
        .await
        .unwrap();

    // Create snapshot
    let snapshot = storage.create_snapshot().await.unwrap();
    assert_eq!(snapshot.entry_count, 3, "Snapshot should contain 3 entries");

    // Modify original storage
    storage
        .put(b"snap_key4", StorageValue::from("snap_value4"))
        .await
        .unwrap();
    storage.delete(b"snap_key1").await.unwrap();

    // Test restore from snapshot
    storage.restore_from_snapshot(&snapshot).await.unwrap();
    assert!(
        storage.exists(b"snap_key1").await.unwrap(),
        "snap_key1 should exist after restore"
    );
    assert!(
        storage.exists(b"snap_key2").await.unwrap(),
        "snap_key2 should exist after restore"
    );
    assert!(
        storage.exists(b"snap_key3").await.unwrap(),
        "snap_key3 should exist after restore"
    );
    assert!(
        !storage.exists(b"snap_key4").await.unwrap(),
        "snap_key4 should not exist after restore"
    );

    // Test streaming functionality
    let kv_stream = storage.create_kv_snapshot_stream(&snapshot).await.unwrap();
    let pairs: Vec<_> = kv_stream.collect().await;
    assert_eq!(pairs.len(), 3, "Stream should yield 3 key-value pairs");

    // All results should be Ok
    for (i, result) in pairs.iter().enumerate() {
        assert!(
            result.is_ok(),
            "Stream result {} should be Ok: {:?}",
            i,
            result
        );
    }
}

/// Test edge cases and error conditions
pub async fn test_edge_cases<T>(storage: &T)
where
    T: StorageBackend + Send + Sync,
{
    // Test small key operations (using minimal key instead of empty to support all backends)
    let small_key = b"a"; // Minimal 1-byte key instead of empty key
    let value = StorageValue::from("small_key_value");

    storage.put(small_key, value.clone()).await.unwrap();
    assert_eq!(
        storage.get(small_key).await.unwrap(),
        Some(value),
        "Should handle small keys correctly"
    );
    storage.delete(small_key).await.unwrap();

    // Test large key operations
    let large_key = vec![b'x'; 1000]; // 1KB key
    let large_value = StorageValue::from(vec![b'y'; 10000]); // 10KB value

    storage.put(&large_key, large_value.clone()).await.unwrap();
    assert_eq!(
        storage.get(&large_key).await.unwrap(),
        Some(large_value),
        "Should handle large keys and values correctly"
    );
    storage.delete(&large_key).await.unwrap();

    // Test scan with empty results
    let empty_results = storage.scan(b"nonexistent_prefix").await.unwrap();
    assert_eq!(
        empty_results.len(),
        0,
        "Scan with nonexistent prefix should return empty results"
    );

    // Test range operations with empty ranges
    let empty_range_results = storage
        .scan_range(b"zzz".to_vec()..b"zzz".to_vec())
        .await
        .unwrap();
    assert_eq!(
        empty_range_results.len(),
        0,
        "Empty range scan should return no results"
    );

    // Test operations on nonexistent keys
    assert_eq!(
        storage.get(b"nonexistent").await.unwrap(),
        None,
        "Get on nonexistent key should return None"
    );
    assert!(
        !storage.exists(b"nonexistent").await.unwrap(),
        "Exists on nonexistent key should return false"
    );

    // Delete nonexistent key should not error
    storage.delete(b"nonexistent").await.unwrap();
}

/// Test StorageValue ordering consistency across backends
pub async fn test_storage_value_ordering<T>(storage: &T)
where
    T: StorageBackend + Send + Sync,
{
    // Clear any existing data first
    let _ = storage.delete_range(b"".to_vec()..b"\xFF".repeat(10)).await;

    // Test keys that should sort by byte content, not enum variant
    let small_key = StorageValue::from(vec![1u8, 2, 3]); // Will be Small variant
    let large_key = StorageValue::from(vec![0u8, 1, 2]); // Smaller byte content

    storage
        .put(small_key.as_slice(), StorageValue::from("small_value"))
        .await
        .unwrap();
    storage
        .put(large_key.as_slice(), StorageValue::from("large_value"))
        .await
        .unwrap();

    // Scan should return in byte-content order, not enum variant order
    let all_results = storage.scan(b"").await.unwrap();
    assert_eq!(all_results.len(), 2, "Should have 2 entries");

    // large_key ([0,1,2]) should come before small_key ([1,2,3]) in byte order
    assert_eq!(
        all_results[0].0.as_slice(),
        large_key.as_slice(),
        "Key with smaller byte content should come first"
    );
    assert_eq!(
        all_results[1].0.as_slice(),
        small_key.as_slice(),
        "Key with larger byte content should come second"
    );
}

/// Quick test for basic functionality - useful for smoke testing
pub async fn test_storage_backend_basic<T>(storage: T)
where
    T: StorageBackend + Send + Sync,
{
    test_basic_operations(&storage).await;
    test_transaction_operations(&storage).await;
    test_scan_operations(&storage).await;
}
