use flare_dht::{error::FlareError, shard::ShardId};
use scc::HashMap;

use crate::error::OdgmError;

use super::{
    ObjectEntry, ObjectShard, ShardFactory, ShardMetadata,
};

type ObjectShardFactory = Box<dyn ShardFactory<Key = u64, Entry = ObjectEntry>>;

pub struct ShardManager {
    pub shard_factory: ObjectShardFactory,
    shards: HashMap<ShardId, ObjectShard>,
}

impl ShardManager {
    pub fn new(shard_factory: ObjectShardFactory) -> Self {
        Self {
            shards: HashMap::new(),
            shard_factory,
        }
    }

    #[inline]
    pub fn get_shard(
        &self,
        shard_id: ShardId,
    ) -> Result<ObjectShard, FlareError> {
        self.shards
            .get(&shard_id)
            .map(|shard| shard.get().to_owned())
            .ok_or_else(|| FlareError::NoShardFound(shard_id))
    }

    #[inline]
    pub fn get_any_shard(
        &self,
        shard_ids: &Vec<ShardId>,
    ) -> Result<ObjectShard, FlareError> {
        for id in shard_ids.iter() {
            if let Some(shard) =
                self.shards.get(id).map(|shard| shard.get().to_owned())
            {
                return Ok(shard);
            }
        }
        Err(FlareError::NoShardsFound(shard_ids.clone()))
    }

    pub async fn create_shard(
        &self,
        shard_metadata: ShardMetadata,
    ) -> Result<(), OdgmError> {
        let shard = self.shard_factory.create_shard(shard_metadata).await?;
        shard.initialize().await?;
        let shard_id = shard.meta().id;
        self.shards.upsert(shard_id, shard);
        Ok(())
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
            if let Err(err) = self.create_shard(s.to_owned()).await {
                tracing::error!("create shard {:?}: failed: {:?}", s, err);
            };
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
            if let Some((k, shard)) = self.shards.remove(entry.key()) {
                if let Err(err) = shard.close().await {
                    tracing::error!("close shard {:?}: failed: {:?}", k, err);
                };
            }
            iter = entry.next_async().await;
        }
    }
}
