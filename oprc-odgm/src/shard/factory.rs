use tracing::info;

use crate::{
    error::OdgmError,
    shard::{raft, BasicObjectShard, ObjectMstShard, ObjectShard},
};
use std::sync::Arc;

use super::{ObjectEntry, ShardFactory, ShardMetadata};

pub struct UnifyShardFactory {
    session_pool: oprc_zenoh::pool::Pool,
}

impl UnifyShardFactory {
    pub fn new(session_pool: oprc_zenoh::pool::Pool) -> Self {
        Self { session_pool }
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
        ObjectShard::new(Arc::new(shard), z_session)
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
        ObjectShard::new(Arc::new(shard), z_session)
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
            Ok(ObjectShard::new(Arc::new(shard), z_session))
        }
    }
}
