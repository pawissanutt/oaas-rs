use std::sync::Arc;

use flare_dht::{
    error::FlareError,
    shard::{KvShard, ShardFactory2, ShardId, ShardMetadata},
};
use scc::HashMap;

pub struct ShardManager<K, V>
where
    K: Send + Clone,
    V: Send + Sync + Default,
{
    pub shard_factory: Box<dyn ShardFactory2<Key = K, Entry = V>>,
    pub shards: HashMap<ShardId, Arc<dyn KvShard<Key = K, Entry = V>>>,
}

impl<K, V> ShardManager<K, V>
where
    K: Send + Clone,
    V: Send + Sync + Default,
{
    pub fn new(
        shard_factory: Box<dyn ShardFactory2<Key = K, Entry = V>>,
    ) -> Self {
        Self {
            shards: HashMap::new(),
            shard_factory,
        }
    }

    #[inline]
    pub fn get_shard(
        &self,
        shard_id: ShardId,
    ) -> Result<Arc<dyn KvShard<Key = K, Entry = V>>, FlareError> {
        self.shards
            .get(&shard_id)
            .map(|shard| shard.get().to_owned())
            .ok_or_else(|| FlareError::NoShardFound(shard_id))
    }

    #[inline]
    pub fn get_any_shard(
        &self,
        shard_ids: &Vec<ShardId>,
    ) -> Result<Arc<dyn KvShard<Key = K, Entry = V>>, FlareError> {
        for id in shard_ids.iter() {
            if let Some(shard) =
                self.shards.get(id).map(|shard| shard.get().to_owned())
            {
                return Ok(shard);
            }
        }
        Err(FlareError::NoShardsFound(shard_ids.clone()))
    }

    #[inline]
    pub async fn create_shard(&self, shard_metadata: ShardMetadata) {
        let shard = self.shard_factory.create_shard(shard_metadata).await;
        let shard_id = shard.meta().id;
        shard.initialize().await.unwrap();
        self.shards.upsert(shard_id, shard);
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
    }
}
