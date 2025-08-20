//! Tests for Fjall storage backend

use super::*;
use crate::tests::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageBackend, StorageConfig, StorageValue};
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
    async fn test_fjall_storage_comprehensive() {
        let (storage, _temp_dir) = create_test_storage();
        test_storage_backend_comprehensive(storage).await;
    }

    #[tokio::test]
    async fn test_fjall_storage_value_ordering() {
        let (storage, _temp_dir) = create_test_storage();
        test_storage_value_ordering(&storage).await;
    }

    #[tokio::test]
    async fn test_fjall_storage_basic() {
        let (storage, _temp_dir) = create_test_storage();
        test_storage_backend_basic(storage).await;
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
}
