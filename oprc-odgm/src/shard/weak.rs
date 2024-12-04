use std::sync::Arc;

use oprc_zenoh::ServiceIdentifier;
use scc::{
    hash_map::Entry::{Occupied, Vacant},
    HashMap,
};

use flare_dht::{
    error::FlareError,
    shard::{KvShard, ShardFactory, ShardMetadata},
};
use merkle_search_tree::MerkleSearchTree;
use tokio::sync::{mpsc::UnboundedSender, RwLock};
use tracing::info;

use crate::shard::event::ZenohEventPublisher;

use super::{event::ObjectChangedEvent, ObjectEntry};

pub struct WeakObjectShardFactory {
    z_session: zenoh::Session,
}

impl ShardFactory<ObjectMstShard> for WeakObjectShardFactory {
    fn create_shard(
        &self,
        shard_metadata: ShardMetadata,
    ) -> Arc<ObjectMstShard> {
        info!("create shard {:?}", &shard_metadata);
        let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
        let shard = ObjectMstShard {
            shard_metadata: shard_metadata,
            map: HashMap::new(),
            sender,
            mst: RwLock::new(MerkleSearchTree::default()),
        };
        let shard = Arc::new(shard);
        let e_pub = ZenohEventPublisher::new(
            ServiceIdentifier {
                ..Default::default()
            },
            self.z_session.clone(),
        );
        e_pub.pipe(receiver);
        shard
    }
}

#[derive(Debug)]
pub struct ObjectMstShard {
    pub shard_metadata: ShardMetadata,
    pub map: HashMap<u64, ObjectEntry>,
    pub mst: RwLock<MerkleSearchTree<u64, ObjectEntry>>,
    sender: UnboundedSender<ObjectChangedEvent>,
}

#[async_trait::async_trait]
impl KvShard for ObjectMstShard {
    type Key = u64;
    type Entry = ObjectEntry;

    fn meta(&self) -> &ShardMetadata {
        &self.shard_metadata
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, FlareError> {
        let out = self.map.get_async(key).await;
        let out = out.map(|r| r.clone());
        Ok(out)
    }

    async fn modify<F, O>(&self, key: &Self::Key, f: F) -> Result<O, FlareError>
    where
        F: FnOnce(&mut Self::Entry) -> O + Send,
    {
        let out = match self.map.entry_async(key.clone()).await {
            Occupied(mut occupied_entry) => {
                let entry = occupied_entry.get_mut();
                let o = f(entry);
                self.sender
                    .send(ObjectChangedEvent::Update(entry.clone()))
                    .map_err(FlareError::from)?;
                o
            }
            Vacant(vacant_entry) => {
                let mut entry = Self::Entry::default();
                let o = f(&mut entry);
                let cloned = entry.clone();
                vacant_entry.insert_entry(entry);
                self.sender
                    .send(ObjectChangedEvent::Update(cloned))
                    .map_err(FlareError::from)?;
                o
            }
        };
        Ok(out)
    }

    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), FlareError> {
        let mut mst = __self.mst.write().await;
        mst.upsert(key, &value);
        drop(mst);
        let copied = value.clone();
        self.map.upsert_async(key, value).await;
        self.sender
            .send(ObjectChangedEvent::Update(copied))
            .map_err(FlareError::from)?;
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), FlareError> {
        self.map.remove_async(key).await;
        self.sender
            .send(ObjectChangedEvent::Delete(*key))
            .map_err(FlareError::from)?;
        Ok(())
    }
}
