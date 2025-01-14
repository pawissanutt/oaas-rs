use flare_dht::error::FlareError;
use oprc_zenoh::OprcZenohConfig;
use scc::HashMap;
use tokio::sync::Mutex;
use tracing::info;

use crate::{
    shard::{raft, ObjectMstShard, ObjectShard, Shard},
    OdgmConfig,
};
use std::sync::Arc;

use super::{ObjectEntry, ShardFactory};

pub struct UnifyShardFactory {
    z_conf: OprcZenohConfig,
    max_sessions: u16,
    session_pool: Arc<Mutex<Pool>>,
}

struct Pool(Vec<zenoh::Session>, usize);

impl UnifyShardFactory {
    pub fn new(z_conf: OprcZenohConfig, odgm_conf: OdgmConfig) -> Self {
        Self {
            z_conf,
            max_sessions: odgm_conf.max_sessions,
            session_pool: Arc::new(Mutex::new(Pool(vec![], 0))),
        }
    }

    async fn get_session(
        &self,
    ) -> Result<zenoh::Session, Box<dyn std::error::Error + Send + Sync>> {
        // let session = zenoh::open(self.z_conf.create_zenoh()).await?;
        // Ok(session)
        let mut pool = self.session_pool.lock().await;
        pool.1 += 1;
        if pool.0.len() < self.max_sessions as usize {
            let session = zenoh::open(self.z_conf.create_zenoh()).await?;
            pool.0.push(session);
        }
        Ok(pool.0[pool.1 % pool.0.len()].clone())
    }

    async fn create_raft(
        &self,
        z_session: zenoh::Session,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Shard {
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
        Shard::new(Arc::new(shard), z_session)
    }

    async fn create_weak(
        &self,
        z_session: zenoh::Session,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Shard {
        info!("create weak shard {:?}", &shard_metadata);
        let shard = ObjectMstShard::new(z_session.clone(), shard_metadata);
        Shard::new(Arc::new(shard), z_session)
    }
}

#[async_trait::async_trait]
impl ShardFactory for UnifyShardFactory {
    type Key = u64;

    type Entry = ObjectEntry;
    async fn create_shard(
        &self,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Result<Shard, FlareError> {
        tracing::info!("create shard {:?}", &shard_metadata);
        let z_session = self
            .get_session()
            .await
            .map_err(|e| FlareError::UnknownError(e))?;
        if shard_metadata.shard_type.eq("raft") {
            Ok(self.create_raft(z_session, shard_metadata).await)
        } else if shard_metadata.shard_type.eq("weak") {
            Ok(self.create_weak(z_session, shard_metadata).await)
        } else {
            let shard = ObjectShard {
                shard_metadata: shard_metadata,
                map: HashMap::new(),
            };
            Ok(Shard::new(Arc::new(shard), z_session))
        }
    }
}
