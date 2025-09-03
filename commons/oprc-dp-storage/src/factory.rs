use crate::{StorageBackendType, StorageConfig, StorageError, StorageResult};

/// Factory for creating storage backends
pub struct StorageFactory;

impl StorageFactory {
    /// Create a memory storage backend (always available)
    pub async fn create_memory()
    -> StorageResult<crate::backends::memory::MemoryStorage> {
        let config = StorageConfig::memory();
        crate::backends::memory::MemoryStorage::new(config)
    }

    /// Create a temporary storage backend for testing
    pub async fn create_temp()
    -> StorageResult<crate::backends::memory::MemoryStorage> {
        Self::create_memory().await
    }

    /// List available backend types
    pub fn available_backends() -> Vec<StorageBackendType> {
        let mut backends = vec![];

        #[cfg(feature = "memory")]
        backends.push(StorageBackendType::Memory);

        #[cfg(feature = "redb")]
        backends.push(StorageBackendType::Redb);

        #[cfg(feature = "fjall")]
        backends.push(StorageBackendType::Fjall);

        #[cfg(feature = "rocksdb")]
        backends.push(StorageBackendType::RocksDb);

        backends
    }

    /// Check if a backend type is available
    pub fn is_backend_available(backend_type: &StorageBackendType) -> bool {
        Self::available_backends().contains(backend_type)
    }
}

/// Builder pattern for creating storage backends with fluent API
pub struct StorageBuilder {
    config: StorageConfig,
}

impl StorageBuilder {
    /// Create a new storage builder
    pub fn new(backend_type: StorageBackendType) -> Self {
        Self {
            config: StorageConfig {
                backend_type,
                ..Default::default()
            },
        }
    }

    /// Create a memory storage builder
    pub fn memory() -> Self {
        Self::new(StorageBackendType::Memory)
    }

    /// Create a redb storage builder
    pub fn redb<P: Into<String>>(path: P) -> Self {
        Self {
            config: StorageConfig::redb(path),
        }
    }

    /// Create a fjall storage builder
    pub fn fjall<P: Into<String>>(path: P) -> Self {
        Self {
            config: StorageConfig::fjall(path),
        }
    }

    /// Create a rocksdb storage builder
    pub fn rocksdb<P: Into<String>>(path: P) -> Self {
        Self {
            config: StorageConfig::rocksdb(path),
        }
    }

    /// Set memory limit
    pub fn memory_limit(mut self, limit_mb: usize) -> Self {
        self.config = self.config.with_memory_limit(limit_mb);
        self
    }

    /// Set cache size
    pub fn cache_size(mut self, cache_mb: usize) -> Self {
        self.config = self.config.with_cache_size(cache_mb);
        self
    }

    /// Enable compression
    pub fn with_compression(mut self) -> Self {
        self.config = self.config.with_compression(true);
        self
    }

    /// Disable sync writes (for better performance, less durability)
    pub fn async_writes(mut self) -> Self {
        self.config = self.config.with_sync_writes(false);
        self
    }

    /// Add a custom property
    pub fn property<K: Into<String>, V: Into<String>>(
        mut self,
        key: K,
        value: V,
    ) -> Self {
        self.config = self.config.with_property(key, value);
        self
    }

    /// Build the storage backend (currently only supports memory)
    pub async fn build(
        self,
    ) -> StorageResult<crate::backends::memory::MemoryStorage> {
        // For now, only support memory storage in the PoC
        match self.config.backend_type {
            StorageBackendType::Memory => {
                crate::backends::memory::MemoryStorage::new(self.config)
            }
            _ => Err(StorageError::configuration(
                "Only memory backend supported in PoC",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageBackend;

    #[test]
    fn test_available_backends() {
        let backends = StorageFactory::available_backends();
        assert!(!backends.is_empty());

        // Memory should always be available
        assert!(backends.contains(&StorageBackendType::Memory));
        assert!(StorageFactory::is_backend_available(
            &StorageBackendType::Memory
        ));
    }

    #[test]
    fn test_storage_builder() {
        let builder = StorageBuilder::memory()
            .memory_limit(256)
            .cache_size(32)
            .with_compression()
            .async_writes()
            .property("test_key", "test_value");

        assert_eq!(builder.config.backend_type, StorageBackendType::Memory);
        assert_eq!(builder.config.memory_limit_mb, Some(256));
        assert_eq!(builder.config.cache_size_mb, Some(32));
        assert!(builder.config.compression);
        assert!(!builder.config.sync_writes);
        assert_eq!(
            builder.config.properties.get("test_key"),
            Some(&"test_value".to_string())
        );
    }

    #[tokio::test]
    async fn test_create_memory_storage() {
        let result = StorageFactory::create_memory().await;
        assert!(result.is_ok());

        let storage = result.unwrap();
        assert_eq!(storage.backend_type(), StorageBackendType::Memory);
    }
}
