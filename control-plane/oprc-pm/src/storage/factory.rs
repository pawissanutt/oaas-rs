use crate::config::StorageConfig;
use anyhow::Result;
use oprc_cp_storage::memory::MemoryStorageFactory;

pub async fn create_storage_factory(config: &StorageConfig) -> Result<MemoryStorageFactory> {
    match config.storage_type {
        crate::config::StorageType::Etcd => {
            // TODO: Implement etcd storage factory
            todo!("Etcd storage factory not implemented yet")
        }
        crate::config::StorageType::Memory => {
            Ok(MemoryStorageFactory)
        }
    }
}
