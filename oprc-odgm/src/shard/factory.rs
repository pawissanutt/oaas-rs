use tracing::info;

use crate::{
    error::OdgmError,
    events::{EventConfig, EventManager, TriggerProcessor},
    shard::{raft, BasicObjectShard, ObjectMstShard, ObjectShard},
};
use std::sync::Arc;

use super::{ObjectEntry, ShardFactory, ShardMetadata};

pub struct UnifyShardFactory {
    session_pool: oprc_zenoh::pool::Pool,
    event_config: Option<EventConfig>,
}

impl UnifyShardFactory {
    pub fn new(session_pool: oprc_zenoh::pool::Pool) -> Self {
        Self {
            session_pool,
            event_config: None,
        }
    }

    pub fn new_with_events(
        session_pool: oprc_zenoh::pool::Pool,
        event_config: EventConfig,
    ) -> Self {
        Self {
            session_pool,
            event_config: Some(event_config),
        }
    }

    fn create_event_manager(
        &self,
        z_session: &zenoh::Session,
    ) -> Option<Arc<EventManager>> {
        if let Some(config) = &self.event_config {
            let trigger_processor = Arc::new(TriggerProcessor::new(
                z_session.clone(),
                config.clone(),
            ));
            Some(Arc::new(EventManager::new(trigger_processor)))
        } else {
            None
        }
    }

    async fn create_raft(
        &self,
        z_session: zenoh::Session,
        shard_metadata: ShardMetadata,
    ) -> ObjectShard {
        info!("create raft shard {:?}", &shard_metadata);
        let rpc_prefix = format!(
            "oprc/{}/{}",
            shard_metadata.collection, shard_metadata.partition_id
        );
        let shard = raft::RaftObjectShard::new(
            z_session.clone(),
            rpc_prefix,
            shard_metadata,
        )
        .await;
        let event_manager = self.create_event_manager(&z_session);
        ObjectShard::new(Arc::new(shard), z_session, event_manager)
    }

    async fn create_mst(
        &self,
        z_session: zenoh::Session,
        shard_metadata: ShardMetadata,
    ) -> ObjectShard {
        info!("create weak shard {:?}", &shard_metadata);
        let rpc_prefix = format!(
            "oprc/{}/{}",
            shard_metadata.collection, shard_metadata.partition_id
        );
        let shard =
            ObjectMstShard::new(z_session.clone(), shard_metadata, rpc_prefix);
        let event_manager = self.create_event_manager(&z_session);
        ObjectShard::new(Arc::new(shard), z_session, event_manager)
    }
}

#[async_trait::async_trait]
impl ShardFactory for UnifyShardFactory {
    type Key = u64;
    type Entry = ObjectEntry;

    async fn create_shard(
        &self,
        shard_metadata: ShardMetadata,
    ) -> Result<ObjectShard, OdgmError> {
        let z_session = self.session_pool.get_session().await?;
        if shard_metadata.shard_type.to_lowercase().eq("raft") {
            Ok(self.create_raft(z_session, shard_metadata).await)
        } else if shard_metadata.shard_type.to_lowercase().eq("mst") {
            Ok(self.create_mst(z_session, shard_metadata).await)
        } else {
            tracing::info!("create basic shard {:?}", &shard_metadata);
            let shard = BasicObjectShard::new(shard_metadata);
            let event_manager = self.create_event_manager(&z_session);
            Ok(ObjectShard::new(Arc::new(shard), z_session, event_manager))
        }
    }
}
