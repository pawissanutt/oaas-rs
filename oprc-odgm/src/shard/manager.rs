use std::{ops::Deref, sync::Arc};

use flare_dht::{
    error::FlareError,
    shard::{KvShard, ShardFactory2, ShardId, ShardMetadata},
};
use scc::HashMap;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::{network::ShardNetwork, ObjectEntry};

type ObjectShard = Arc<dyn KvShard<Key = u64, Entry = ObjectEntry>>;
type ObjectShardFactory =
    Box<dyn ShardFactory2<Key = u64, Entry = ObjectEntry>>;

#[derive(Clone)]
struct ShardWrapper {
    shard: ObjectShard,
    network: ShardNetwork,
    token: CancellationToken,
}

impl ShardWrapper {
    fn sync_network(&self) {
        let mut wrapper = self.clone();
        tokio::spawn(async move {
            let mut receiver = wrapper.shard.watch_readiness();
            loop {
                tokio::select! {
                    _ = receiver.changed() => {
                        if receiver.borrow().to_owned() {
                            if !wrapper.network.is_running() {
                                info!("Start network for shard {}", wrapper.shard.meta().id);
                                if let Err(e) = wrapper.network.start().await {
                                    error!("Failed to start network: {}", e);
                                }
                            }
                        } else {
                            if wrapper.network.is_running() {
                                wrapper.network.stop();
                            }
                        }
                    }
                    _ = wrapper.token.cancelled() => {
                        break;
                    }
                }
            }
        });
    }

    async fn close(&self) -> Result<(), FlareError> {
        self.token.cancel();
        if self.network.is_running() {
            self.network.stop();
        }
        self.shard.close().await
    }
}

impl Deref for ShardWrapper {
    type Target = ObjectShard;

    fn deref(&self) -> &Self::Target {
        &self.shard
    }
}

pub struct ShardManager {
    pub shard_factory: ObjectShardFactory,
    shards: HashMap<ShardId, ShardWrapper>,
    z_session: zenoh::Session,
}

impl ShardManager {
    pub fn new(
        shard_factory: ObjectShardFactory,
        z_session: zenoh::Session,
    ) -> Self {
        Self {
            shards: HashMap::new(),
            shard_factory,
            z_session,
        }
    }

    #[inline]
    pub fn get_shard(
        &self,
        shard_id: ShardId,
    ) -> Result<ObjectShard, FlareError> {
        self.shards
            .get(&shard_id)
            .map(|shard| shard.get().shard.to_owned())
            .ok_or_else(|| FlareError::NoShardFound(shard_id))
    }

    #[inline]
    pub fn get_any_shard(
        &self,
        shard_ids: &Vec<ShardId>,
    ) -> Result<ObjectShard, FlareError> {
        for id in shard_ids.iter() {
            if let Some(shard) = self
                .shards
                .get(id)
                .map(|shard| shard.get().shard.to_owned())
            {
                return Ok(shard);
            }
        }
        Err(FlareError::NoShardsFound(shard_ids.clone()))
    }

    #[inline]
    pub async fn create_shard(&self, shard_metadata: ShardMetadata) {
        let prefix = format!(
            "oprc/{}/{}/objects",
            shard_metadata.collection.clone(),
            shard_metadata.partition_id,
        );
        let shard = self.shard_factory.create_shard(shard_metadata).await;
        let shard_id = shard.meta().id;
        shard.initialize().await.unwrap();
        let wrapper = ShardWrapper {
            shard: shard.clone(),
            network: ShardNetwork::new(self.z_session.clone(), shard, prefix),
            token: CancellationToken::new(),
        };
        wrapper.sync_network();
        self.shards.upsert(shard_id, wrapper);
    }

    #[inline]
    pub fn contains(&self, shard_id: ShardId) -> bool {
        self.shards.contains(&shard_id)
    }

    pub async fn sync_shards(&self, shard_meta: &Vec<ShardMetadata>) {
        for s in shard_meta {
            if self.contains(s.id) {
                continue;
            }
            self.create_shard(s.to_owned()).await;
        }
    }

    pub async fn remove_shard(&self, shard_id: ShardId) {
        if let Some((_, v)) = self.shards.remove(&shard_id) {
            let _ = v.close().await;
        }
    }

    pub async fn close(&self) {
        let mut iter = self.shards.first_entry_async().await;
        while let Some(entry) = iter {
            entry.close().await.expect("close shard failed");
            iter = entry.next_async().await;
        }
        self.z_session.close().await.unwrap();
    }
}
