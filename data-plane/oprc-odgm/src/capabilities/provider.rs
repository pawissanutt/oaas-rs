use std::sync::Arc;

use crate::cluster::ObjectDataGridManager;
use crate::shard::ShardId;

use super::model::{CapErr, Features, ShardCapabilities};

#[derive(Clone)]
pub struct CapabilitiesProvider {
    odgm: Arc<ObjectDataGridManager>,
    version: String,
    // Static feature flags (env-driven)
    event_pipeline_v2: bool,
    bridge_mode: bool,
}

impl CapabilitiesProvider {
    pub fn new(
        odgm: Arc<ObjectDataGridManager>,
        event_pipeline_v2: bool,
        bridge_mode: bool,
        version: String,
    ) -> Self {
        Self {
            odgm,
            version,
            event_pipeline_v2,
            bridge_mode,
        }
    }

    pub async fn get_shard_capabilities(
        &self,
        cls: &str,
        partition: u32,
        shard_id: ShardId,
    ) -> Result<ShardCapabilities, CapErr> {
        // Locate local shard by (class, partition) and shard_id
        let exists = self
            .odgm
            .shard_manager
            .get_shard(shard_id)
            .map(|s| {
                s.meta().collection == cls
                    && s.meta().partition_id as u32 == partition
            })
            .unwrap_or(false);
        if !exists {
            return Err(CapErr::NotFound);
        }

        // Determine storage backend from metadata
        let storage_backend = self
            .odgm
            .shard_manager
            .get_shard(shard_id)
            .map(|s| {
                backend_type_to_str(&s.meta().storage_backend_type())
                    .to_string()
            })
            .unwrap_or_else(|| "unknown".to_string());

        let caps = ShardCapabilities {
            class: cls.to_string(),
            partition,
            shard_id,
            event_pipeline_v2: self.event_pipeline_v2,
            storage_backend,
            odgm_version: self.version.clone(),
            features: Features {
                granular_storage: true,
                bridge_mode: self.bridge_mode,
            },
        };
        Ok(caps)
    }
}

// Helper to display backend type
fn backend_type_to_str(
    b: &oprc_dp_storage::StorageBackendType,
) -> &'static str {
    match b {
        oprc_dp_storage::StorageBackendType::Memory => "memory",
        oprc_dp_storage::StorageBackendType::Fjall => "fjall",
        oprc_dp_storage::StorageBackendType::Redb => "redb",
        oprc_dp_storage::StorageBackendType::RocksDb => "rocksdb",
        _ => "unknown",
    }
}
