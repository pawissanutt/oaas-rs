use crate::StorageValue;

/// Storage backend types
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StorageBackendType {
    Memory,
    Redb,
    Fjall,
    RocksDb,
}

impl std::fmt::Display for StorageBackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, "memory"),
            Self::Redb => write!(f, "redb"),
            Self::Fjall => write!(f, "fjall"),
            Self::RocksDb => write!(f, "rocksdb"),
        }
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub entries_count: u64,
    pub total_size_bytes: u64,
    pub memory_usage_bytes: Option<u64>,
    pub disk_usage_bytes: Option<u64>,
    pub cache_hit_rate: Option<f64>,
    pub backend_specific: std::collections::HashMap<String, String>,
}

impl Default for StorageStats {
    fn default() -> Self {
        Self {
            entries_count: 0,
            total_size_bytes: 0,
            memory_usage_bytes: None,
            disk_usage_bytes: None,
            cache_hit_rate: None,
            backend_specific: std::collections::HashMap::new(),
        }
    }
}

/// Batch operation for efficient bulk operations
#[derive(Debug, Clone)]
pub enum BatchOperation {
    Put {
        key: StorageValue,
        value: StorageValue,
    },
    Delete {
        key: StorageValue,
    },
}

/// Storage snapshot for backup/restore operations
#[derive(Debug, Clone)]
pub struct StorageSnapshot {
    pub timestamp: std::time::SystemTime,
    pub data: StorageValue, // Changed from Vec<u8> to StorageValue
    pub metadata: std::collections::HashMap<String, String>,
}

/// Backend-specific configuration
#[derive(Debug, Clone)]
pub struct StorageBackendConfig {
    pub backend_type: StorageBackendType,
    pub properties: std::collections::HashMap<String, String>,
}

/// Compression types supported by storage backends
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum CompressionType {
    None,
    Gzip,
    Lz4,
    Zstd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_display() {
        assert_eq!(StorageBackendType::Memory.to_string(), "memory");
        assert_eq!(StorageBackendType::Redb.to_string(), "redb");
        assert_eq!(StorageBackendType::Fjall.to_string(), "fjall");
        assert_eq!(StorageBackendType::RocksDb.to_string(), "rocksdb");
    }

    #[test]
    fn test_batch_operation_with_storage_value() {
        let key = StorageValue::from("test_key");
        let value = StorageValue::from("test_value");

        let put_op = BatchOperation::Put {
            key: key.clone(),
            value: value.clone(),
        };
        let delete_op = BatchOperation::Delete { key };

        match put_op {
            BatchOperation::Put { key: _, value: _ } => (),
            _ => panic!("Expected Put operation"),
        }

        match delete_op {
            BatchOperation::Delete { key: _ } => (),
            _ => panic!("Expected Delete operation"),
        }
    }

    #[test]
    fn test_storage_snapshot_with_storage_value() {
        let data = StorageValue::from("snapshot_data");
        let snapshot = StorageSnapshot {
            timestamp: std::time::SystemTime::now(),
            data: data.clone(),
            metadata: std::collections::HashMap::new(),
        };

        assert_eq!(snapshot.data, data);
    }
}
