use std::collections::{BTreeMap, HashMap};

use flare_dht::{
    error::FlareError,
    metadata::{state_machine::CollectionMetadata, MetadataManager},
    shard::ShardMetadata,
};
use flare_pb::{
    ClusterMetadata, CreateCollectionRequest, CreateCollectionResponse,
    JoinRequest, JoinResponse,
};
use tokio::sync::RwLock;

#[derive(Clone, Default, Debug)]
pub struct MemberMetadata {
    pub node_id: u64,
    pub node_addr: String,
}

pub struct OprcMetaManager {
    pub collections: RwLock<BTreeMap<String, CollectionMetadata>>,
    pub shards: RwLock<BTreeMap<u64, ShardMetadata>>,
    pub membership: scc::hash_map::HashMap<u64, MemberMetadata>,
    pub node_id: u64,
    node_addr: String,
    sender: tokio::sync::watch::Sender<u64>,
    receiver: tokio::sync::watch::Receiver<u64>,
}

impl OprcMetaManager {
    pub fn new(node_id: u64, node_addr: String) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(0);
        Self {
            collections: RwLock::new(BTreeMap::new()),
            shards: RwLock::new(BTreeMap::new()),
            membership: scc::hash_map::HashMap::new(),
            node_addr,
            node_id,
            sender: tx,
            receiver: rx,
        }
    }
}

#[async_trait::async_trait]
impl MetadataManager for OprcMetaManager {
    async fn initialize(&self) -> Result<(), FlareError> {
        Ok(())
    }
    async fn get_shard_ids(&self, col_name: &str) -> Option<Vec<u64>> {
        let collections = self.collections.read().await;
        if let Some(col) = collections.get(col_name) {
            return Some(col.shard_ids.clone());
        } else {
            return None;
        }
    }
    async fn get_shard_id(&self, col_name: &str, key: &[u8]) -> Option<u64> {
        let collections = self.collections.read().await;
        if let Some(col) = collections.get(col_name) {
            return Some(resolve_shard_id(col, key));
        } else {
            return None;
        }
    }

    async fn leave(&self) {}

    async fn other_leave(&self, node_id: u64) -> Result<(), FlareError> {
        Ok(())
    }
    async fn other_join(
        &self,
        join_request: JoinRequest,
    ) -> Result<JoinResponse, FlareError> {
        todo!()
    }

    async fn get_metadata(&self) -> Result<ClusterMetadata, FlareError> {
        let collections = self.collections.read().await;
        let shards = self.shards.read().await;
        let mut col_meta = HashMap::with_capacity(collections.len());
        for (name, col) in collections.iter() {
            col_meta.insert(
                name.clone(),
                flare_pb::CollectionMetadata {
                    name: col.name.clone(),
                    shard_ids: col.shard_ids.clone(),
                    replication: col.replication as u32,
                },
            );
        }
        let mut shard_meta = HashMap::with_capacity(shards.len());
        for (id, shard) in shards.iter() {
            shard_meta.insert(
                *id,
                flare_pb::ShardMetadata {
                    id: shard.id,
                    collection: shard.collection.clone(),
                    primary: shard.primary,
                    replica: shard.replica.clone(),
                },
            );
        }

        let cm = flare_pb::ClusterMetadata {
            collections: col_meta,
            shards: shard_meta,
            ..Default::default()
        };
        Ok(cm)
    }

    async fn local_shards(&self) -> Vec<ShardMetadata> {
        let shards = self.shards.read().await;

        shards
            .values()
            .filter(|shard| shard.primary.unwrap_or(0) == self.node_id)
            .cloned()
            .collect()
    }
    async fn create_collection(
        &self,
        request: CreateCollectionRequest,
    ) -> Result<CreateCollectionResponse, FlareError> {
        let mut collections = self.collections.write().await;
        let name = &request.name;
        let shard_count = request.shard_count;
        if collections.contains_key(name) {
            return Err(FlareError::InvalidArgument(
                "collection already exist".into(),
            ));
        }

        let mut shard_ids = Vec::with_capacity(shard_count as usize);
        for i in 0..shard_count {
            let mut shards = self.shards.write().await;
            let id = shards.last_key_value().map(|e| *e.0).unwrap_or(1);
            let shard_meta = ShardMetadata {
                id: id,
                collection: name.into(),
                primary: Some(request.shard_assignments[i as usize]),
                ..Default::default()
            };
            shard_ids.push(id);
            shards.insert(id, shard_meta);
        }

        let col_meta = CollectionMetadata {
            name: name.into(),
            shard_ids: shard_ids,
            replication: 1,
            ..Default::default()
        };
        collections.insert(name.into(), col_meta.clone());
        let num = *self.receiver.borrow();
        self.sender.send(num + 1);
        Ok(CreateCollectionResponse {
            name: name.into(),
            ..Default::default()
        })
    }

    fn create_watch(&self) -> tokio::sync::watch::Receiver<u64> {
        use tokio_stream::StreamExt;
        let (tx, rx) = tokio::sync::watch::channel(0);
        let mut stream =
            tokio_stream::wrappers::WatchStream::new(self.receiver.clone());
        tokio::spawn(async move {
            loop {
                if let Some(d) = stream.next().await {
                    if let Err(_) = tx.send(d) {
                        break;
                    }
                }
            }
        });
        rx
    }
}

fn resolve_shard_id(meta: &CollectionMetadata, key: &[u8]) -> u64 {
    let hashed = mur3::murmurhash3_x86_32(key, meta.seed) as u32;
    let shard_count = meta.shard_ids.len();
    let size = u32::div_ceil(u32::MAX, shard_count as u32);
    let shard_index = hashed / size;
    meta.shard_ids[shard_index as usize]
}
