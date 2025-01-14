mod basic;
mod event;
pub mod manager;
pub(crate) mod msg;
mod network;
// mod proxy;
mod invocation;
mod raft;
mod weak;

use std::sync::Arc;

use automerge::AutomergeError;
pub use basic::ObjectEntry;
pub use basic::ObjectShard;
pub use basic::ObjectVal;
use flare_dht::shard::KvShard;
use flare_dht::shard::ShardFactory2;
use scc::HashMap;
use tracing::info;
use weak::ObjectMstShard;

#[derive(thiserror::Error, Debug)]
pub enum ShardError {
    #[error("Merge Error: {0}")]
    MergeError(#[from] AutomergeError),
}

pub struct UnifyShardFactory {
    z_session: zenoh::Session,
}

impl UnifyShardFactory {
    pub fn new(z_session: zenoh::Session) -> Self {
        Self { z_session }
    }

    async fn create_raft(
        &self,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Arc<dyn KvShard<Key = u64, Entry = ObjectEntry>> {
        info!("create raft shard {:?}", &shard_metadata);
        let rpc_prefix = format!(
            "oprc/{}/{}",
            shard_metadata.collection, shard_metadata.partition_id
        );
        let shard = raft::RaftObjectShard::new(
            self.z_session.clone(),
            rpc_prefix,
            shard_metadata,
        )
        .await;
        Arc::new(shard)
    }

    async fn create_weak(
        &self,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Arc<dyn KvShard<Key = u64, Entry = ObjectEntry>> {
        info!("create weak shard {:?}", &shard_metadata);
        let shard = ObjectMstShard::new(self.z_session.clone(), shard_metadata);
        Arc::new(shard)
    }
}

#[async_trait::async_trait]
impl ShardFactory2 for UnifyShardFactory {
    type Key = u64;

    type Entry = ObjectEntry;
    async fn create_shard(
        &self,
        shard_metadata: flare_dht::shard::ShardMetadata,
    ) -> Arc<dyn KvShard<Key = Self::Key, Entry = Self::Entry>> {
        tracing::info!("create shard {:?}", &shard_metadata);
        if shard_metadata.shard_type.eq("raft") {
            self.create_raft(shard_metadata).await
        } else if shard_metadata.shard_type.eq("weak") {
            self.create_weak(shard_metadata).await
        } else {
            let shard = ObjectShard {
                shard_metadata: shard_metadata,
                map: HashMap::new(),
            };
            let shard = Arc::new(shard);
            shard
        }
    }
}
