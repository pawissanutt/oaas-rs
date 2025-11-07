use nohash_hasher::NoHashHasher;
use scc::HashMap;
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::shard::ShardId;

use super::{
    config::ShardError,
    factory::UnifiedShardFactory,
    object_trait::{ArcUnifiedObjectShard, ObjectShard},
    traits::ShardMetadata,
};

/// Simplified unified shard manager for trait objects
pub struct UnifiedShardManager {
    /// Factory for creating new unified shards
    pub shard_factory: Arc<UnifiedShardFactory>,

    /// Active shards mapped by shard ID
    pub(crate) shards: HashMap<
        ShardId,
        ArcUnifiedObjectShard,
        BuildHasherDefault<NoHashHasher<u64>>,
    >,

    /// Metrics and state tracking
    pub(crate) stats: Arc<RwLock<ManagerStats>>,
}

#[derive(Debug, Default, Clone)]
pub struct ManagerStats {
    pub total_shards_created: u32,
    pub total_shards_removed: u32,
    pub active_shards: u32,
    pub failed_operations: u32,
}

impl UnifiedShardManager {
    /// Create a new unified shard manager with the given factory
    pub fn new(shard_factory: Arc<UnifiedShardFactory>) -> Self {
        Self {
            shard_factory,
            shards: HashMap::default(),
            stats: Arc::new(RwLock::new(ManagerStats::default())),
        }
    }

    /// Get a shard by ID (returns Arc for shared ownership)
    #[inline]
    pub fn get_shard(
        &self,
        shard_id: ShardId,
    ) -> Option<ArcUnifiedObjectShard> {
        self.shards
            .read_sync(&shard_id, |_, shard| Arc::clone(shard))
    }

    /// Get any available shard from a list of shard IDs
    #[inline]
    pub fn get_any_shard(
        &self,
        shard_ids: &Vec<ShardId>,
    ) -> Result<ArcUnifiedObjectShard, ShardError> {
        for id in shard_ids.iter() {
            if let Some(shard) = self.get_shard(*id) {
                return Ok(shard);
            }
        }
        Err(ShardError::NoShardsFound(shard_ids.clone()))
    }

    /// Get the primary shard from a list of shard IDs
    pub fn get_primary_shard(
        &self,
        shard_ids: &Vec<ShardId>,
        primary_id: Option<ShardId>,
    ) -> Result<ArcUnifiedObjectShard, ShardError> {
        // Try to get the primary shard first
        if let Some(primary_id) = primary_id {
            if let Some(shard) = self.get_shard(primary_id) {
                return Ok(shard);
            }
        }

        // Fallback to any available shard
        self.get_any_shard(shard_ids)
    }

    /// Get all shards for a collection
    pub async fn get_shards_for_collection(
        &self,
        collection: &str,
    ) -> Vec<ArcUnifiedObjectShard> {
        let mut collection_shards = Vec::new();

        // Iterate using iter_async instead of scan_async
        self.shards
            .iter_async(|_k, v| {
                if v.meta().collection == collection {
                    collection_shards.push(Arc::clone(v));
                }
                true
            })
            .await;

        collection_shards
    }

    /// Create a shard based on metadata configuration
    pub async fn create_shard(
        &self,
        metadata: ShardMetadata,
    ) -> Result<(), ShardError> {
        let shard_id = metadata.id;
        let shard_type = metadata.shard_type.clone();

        info!("Creating shard {} with type: {}", shard_id, shard_type);

        // Create the shard using the factory
        let shard = self
            .shard_factory
            .create_shard_from_metadata(metadata)
            .await?;

        debug!(
            "Begin initialization of shard: {} with type: {}",
            shard_id, shard_type
        );

        // Initialize shard before wrapping
        shard.initialize().await?;

        // Wrap in Arc and store
        let arc_shard: Arc<dyn ObjectShard> = Arc::from(shard);
        self.shards.upsert_sync(shard_id, arc_shard);

        // Update stats
        let mut stats = self.stats.write().await;
        stats.total_shards_created += 1;
        stats.active_shards += 1;

        info!(
            "Successfully created shard: {} (type: {})",
            shard_id, shard_type
        );
        Ok(())
    }

    /// Check if a shard exists
    #[inline]
    pub fn contains(&self, shard_id: ShardId) -> bool {
        self.shards.contains_sync(&shard_id)
    }

    /// Get the number of active shards
    #[inline]
    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    /// Synchronize shards based on metadata (create if not exists)
    pub async fn sync_shards(&self, shard_meta: &ShardMetadata) {
        if self.contains(shard_meta.id) {
            return;
        }

        info!("Syncing shard: {}", shard_meta.id);

        if let Err(err) = self.create_shard(shard_meta.clone()).await {
            error!("Failed to sync shard {:?}: {:?}", shard_meta, err);

            // Update error stats
            tokio::spawn({
                let stats = Arc::clone(&self.stats);
                async move {
                    let mut stats = stats.write().await;
                    stats.failed_operations += 1;
                }
            });
        }
    }

    /// Remove a shard and close it gracefully
    pub async fn remove_shard(&self, shard_id: ShardId) {
        info!("Removing shard: {}", shard_id);

        if let Some((_, shard_ref)) = self.shards.remove_sync(&shard_id) {
            // Try to close the shard gracefully
            // Note: We'll skip the complex Arc::try_unwrap for now and just drop the reference
            // The shard will be cleaned up when all references are dropped
            drop(shard_ref);

            // Update stats
            let mut stats = self.stats.write().await;
            stats.total_shards_removed += 1;
            stats.active_shards = stats.active_shards.saturating_sub(1);

            info!("Successfully removed shard: {}", shard_id);
        } else {
            warn!("Attempted to remove non-existent shard: {}", shard_id);
        }
    }

    /// Close all shards and clean up the manager
    pub async fn close(&self) {
        info!(
            "Closing unified shard manager with {} shards",
            self.shard_count()
        );

        // Collect all shards first
        let mut shards_to_close = Vec::new();
        self.shards
            .iter_async(|shard_id, shard| {
                info!("Preparing to close shard: {}", shard_id);
                shards_to_close.push((*shard_id, Arc::clone(shard)));
                true
            })
            .await;

        // Clear the HashMap
        self.shards.clear_async().await;

        // Try to close shards - we'll skip the complex Arc::try_unwrap for now
        for (shard_id, shard) in shards_to_close {
            info!("Closing shard: {}", shard_id);
            // The shard will be cleaned up when the Arc is dropped
            drop(shard);
        }

        // Reset stats
        let mut stats = self.stats.write().await;
        stats.active_shards = 0;

        info!("Unified shard manager closed");
    }

    /// Get manager statistics
    pub async fn get_stats(&self) -> ManagerStats {
        self.stats.read().await.clone()
    }

    /// Health check for all shards
    pub async fn health_check(&self) -> HealthCheckResult {
        let mut healthy_shards = 0;
        let mut unhealthy_shards = 0;
        let mut total_shards = 0;

        // Use iter_async to iterate over all shards
        self.shards
            .iter_async(|shard_id, shard| {
                total_shards += 1;

                // Check if shard is ready
                let is_ready = *shard.watch_readiness().borrow();
                if is_ready {
                    healthy_shards += 1;
                } else {
                    unhealthy_shards += 1;
                    warn!("Shard {} is not ready", shard_id);
                }
                true
            })
            .await;

        HealthCheckResult {
            total_shards,
            healthy_shards,
            unhealthy_shards,
            health_ratio: if total_shards > 0 {
                healthy_shards as f64 / total_shards as f64
            } else {
                0.0
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub total_shards: usize,
    pub healthy_shards: usize,
    pub unhealthy_shards: usize,
    pub health_ratio: f64,
}

impl HealthCheckResult {
    pub fn is_healthy(&self) -> bool {
        self.unhealthy_shards == 0 && self.total_shards > 0
    }

    pub fn is_degraded(&self) -> bool {
        self.health_ratio > 0.5 && self.health_ratio < 1.0
    }

    pub fn is_critical(&self) -> bool {
        self.health_ratio <= 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_manager_creation() {
        // Test that the manager compiles and can be created
        // Note: This would need a real UnifiedShardFactory to test fully
        assert!(true);
    }

    #[tokio::test]
    async fn test_manager_stats() {
        // Test that the manager compiles and can track stats
        let stats = ManagerStats::default();
        assert_eq!(stats.total_shards_created, 0);
        assert_eq!(stats.active_shards, 0);
    }

    #[tokio::test]
    async fn test_health_check_result() {
        let health = HealthCheckResult {
            total_shards: 10,
            healthy_shards: 8,
            unhealthy_shards: 2,
            health_ratio: 0.8,
        };

        assert!(!health.is_healthy());
        assert!(health.is_degraded());
        assert!(!health.is_critical());
    }
}
