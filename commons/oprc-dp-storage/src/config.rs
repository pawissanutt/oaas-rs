use crate::StorageBackendType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for storage backends
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub backend_type: StorageBackendType,
    pub path: Option<String>,
    pub memory_limit_mb: Option<usize>,
    pub cache_size_mb: Option<usize>,
    pub compression: bool,
    pub sync_writes: bool,
    pub properties: HashMap<String, String>,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend_type: StorageBackendType::Memory,
            path: None,
            memory_limit_mb: Some(100),
            cache_size_mb: Some(16),
            compression: false,
            sync_writes: true,
            properties: HashMap::new(),
        }
    }
}

impl StorageConfig {
    /// Create a memory storage configuration
    pub fn memory() -> Self {
        Self {
            backend_type: StorageBackendType::Memory,
            ..Default::default()
        }
    }

    /// Create a redb storage configuration
    pub fn redb<P: Into<String>>(path: P) -> Self {
        Self {
            backend_type: StorageBackendType::Redb,
            path: Some(path.into()),
            ..Default::default()
        }
    }

    /// Create a fjall storage configuration
    pub fn fjall<P: Into<String>>(path: P) -> Self {
        Self {
            backend_type: StorageBackendType::Fjall,
            path: Some(path.into()),
            ..Default::default()
        }
    }

    /// Create a RocksDB storage configuration
    pub fn rocksdb<P: Into<String>>(path: P) -> Self {
        Self {
            backend_type: StorageBackendType::RocksDb,
            path: Some(path.into()),
            ..Default::default()
        }
    }

    /// Set memory limit in MB
    pub fn with_memory_limit(mut self, limit_mb: usize) -> Self {
        self.memory_limit_mb = Some(limit_mb);
        self
    }

    /// Set cache size in MB
    pub fn with_cache_size(mut self, cache_mb: usize) -> Self {
        self.cache_size_mb = Some(cache_mb);
        self
    }

    /// Enable or disable compression
    pub fn with_compression(mut self, enabled: bool) -> Self {
        self.compression = enabled;
        self
    }

    /// Enable or disable sync writes
    pub fn with_sync_writes(mut self, enabled: bool) -> Self {
        self.sync_writes = enabled;
        self
    }

    /// Add a custom property
    pub fn with_property<K: Into<String>, V: Into<String>>(
        mut self,
        key: K,
        value: V,
    ) -> Self {
        self.properties.insert(key.into(), value.into());
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        match self.backend_type {
            StorageBackendType::Memory => {
                // Memory backend doesn't need a path
                Ok(())
            }
            StorageBackendType::Redb
            | StorageBackendType::Fjall
            | StorageBackendType::RocksDb => {
                if self.path.is_none() {
                    Err(format!(
                        "Path is required for {} backend",
                        self.backend_type
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }
}

/// Replication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    pub replication_type: ReplicationType,
    pub node_id: u64,
    pub peers: Vec<String>,
    pub properties: HashMap<String, String>,
}

/// Types of replication
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplicationType {
    None,
    Raft,
    Mst,
    EventualConsistency,
}

impl std::fmt::Display for ReplicationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Raft => write!(f, "raft"),
            Self::Mst => write!(f, "mst"),
            Self::EventualConsistency => write!(f, "eventual"),
        }
    }
}

/// Consistency configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsistencyConfig {
    pub read_consistency: ReadConsistency,
    pub write_consistency: WriteConsistency,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReadConsistency {
    Eventual,
    Sequential,
    Linearizable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WriteConsistency {
    Async,
    Sync,
    Majority,
    All,
}

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self {
            read_consistency: ReadConsistency::Sequential,
            write_consistency: WriteConsistency::Sync,
            timeout_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_config_builders() {
        let memory_config = StorageConfig::memory();
        assert_eq!(memory_config.backend_type, StorageBackendType::Memory);
        assert!(memory_config.path.is_none());

        let redb_config = StorageConfig::redb("/tmp/test.db")
            .with_memory_limit(200)
            .with_compression(true);
        assert_eq!(redb_config.backend_type, StorageBackendType::Redb);
        assert_eq!(redb_config.path, Some("/tmp/test.db".to_string()));
        assert_eq!(redb_config.memory_limit_mb, Some(200));
        assert!(redb_config.compression);
    }

    #[test]
    fn test_config_validation() {
        let memory_config = StorageConfig::memory();
        assert!(memory_config.validate().is_ok());

        let invalid_redb = StorageConfig {
            backend_type: StorageBackendType::Redb,
            path: None,
            ..Default::default()
        };
        assert!(invalid_redb.validate().is_err());

        let valid_redb = StorageConfig::redb("/tmp/test.db");
        assert!(valid_redb.validate().is_ok());
    }
}
