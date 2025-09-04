//! Tests for Redb storage backend

use super::*;
use crate::tests::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageBackend, StorageConfig, StorageValue};
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper function to create a temporary Redb storage instance
    fn create_test_storage() -> (RedbStorage, TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test.redb");
        let config = StorageConfig::redb(db_path.to_string_lossy());
        let storage =
            RedbStorage::new(config).expect("Failed to create RedbStorage");
        (storage, temp_dir, db_path)
    }

    #[tokio::test]
    async fn test_redb_storage_basic() {
        let (storage, _temp_dir, _db_path) = create_test_storage();
        test_storage_backend_basic(storage).await;
    }

    #[tokio::test]
    async fn test_redb_storage_value_ordering() {
        let (storage, _temp_dir, _db_path) = create_test_storage();
        test_storage_value_ordering(&storage).await;
    }

    #[tokio::test]
    async fn test_redb_storage_range_ops() {
        let (storage, _temp_dir, _db_path) = create_test_storage();
        test_range_operations(&storage).await;
    }

    #[tokio::test]
    async fn test_redb_persistence() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("persist.redb");
        let db_path_str = db_path.to_string_lossy().to_string();

        let key = b"persistent_key";
        let value = StorageValue::from("persistent_value");

        // First storage instance - write data
        {
            let config = StorageConfig::redb(&db_path_str);
            let storage = RedbStorage::new(config).unwrap();
            storage.put(key, value.clone()).await.unwrap();
            // Storage is dropped here
        }

        // Second storage instance - read data
        {
            let config = StorageConfig::redb(&db_path_str);
            let storage = RedbStorage::new(config).unwrap();
            let retrieved = storage.get(key).await.unwrap();
            assert_eq!(retrieved, Some(value));
        }
    }
}
