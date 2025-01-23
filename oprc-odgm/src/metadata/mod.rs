use std::collections::BTreeMap;

use flare_dht::error::FlareError;
use oprc_pb::{
    CreateCollectionRequest, CreateCollectionResponse, ShardAssignment,
};
use tokio::sync::{
    watch::{Receiver, Sender},
    RwLock,
};
use tracing::info;

use crate::shard::ShardMetadata;

pub struct OprcMetaManager {
    pub collections: RwLock<BTreeMap<String, CollectionMetadataState>>,
    pub shards: RwLock<BTreeMap<u64, ShardMetadata>>,
    pub node_id: u64,
    members: Vec<u64>,
    sender: Sender<u64>,
    receiver: Receiver<u64>,
}

impl OprcMetaManager {
    pub fn new(node_id: u64, members: Vec<u64>) -> Self {
        let (tx, rx) = tokio::sync::watch::channel(0);
        info!("use node_id {node_id}");
        Self {
            collections: RwLock::new(BTreeMap::new()),
            shards: RwLock::new(BTreeMap::new()),
            members,
            node_id,
            sender: tx,
            receiver: rx,
        }
    }
}

impl OprcMetaManager {
    pub async fn get_shard_ids(
        &self,
        col_name: &str,
    ) -> Option<Vec<ShardGroupState>> {
        let collections = self.collections.read().await;
        if let Some(col) = collections.get(col_name) {
            return Some(col.shards.clone());
        } else {
            return None;
        }
    }
    pub async fn get_shard_id(
        &self,
        col_name: &str,
        key: &[u8],
    ) -> Option<ShardGroupState> {
        let collections = self.collections.read().await;
        if let Some(col) = collections.get(col_name) {
            return Some(resolve_shard_id(col, key).clone());
        } else {
            return None;
        }
    }

    // pub async fn get_metadata(&self) -> Result<ClusterMetadata, FlareError> {
    //     let collections = self.collections.read().await;
    //     let shards = self.shards.read().await;
    //     let mut col_meta = HashMap::with_capacity(collections.len());
    //     for (name, col) in collections.iter() {
    //         let shard_groups = col
    //             .shards
    //             .iter()
    //             .map(|s| ShardGroup {
    //                 shard_ids: s.shard_ids.clone(),
    //             })
    //             .collect();
    //         col_meta.insert(
    //             name.clone(),
    //             flare_pb::CollectionMetadata {
    //                 name: col.name.clone(),
    //                 shards: shard_groups,
    //                 replication: col.replication as u32,
    //                 options: HashMap::new(),
    //             },
    //         );
    //     }
    //     let mut shard_meta = HashMap::with_capacity(shards.len());
    //     for (id, shard) in shards.iter() {
    //         shard_meta.insert(*id, shard.into_proto());
    //     }

    //     let cm = flare_pb::ClusterMetadata {
    //         collections: col_meta,
    //         shards: shard_meta,
    //         ..Default::default()
    //     };
    //     Ok(cm)
    // }

    pub async fn local_shards(&self) -> Vec<ShardMetadata> {
        let shards = self.shards.read().await;
        shards
            .values()
            .filter(|shard| shard.owner.unwrap_or(0) == self.node_id)
            .cloned()
            .collect()
    }

    pub async fn create_collection(
        &self,
        request: CreateCollectionRequest,
    ) -> Result<CreateCollectionResponse, FlareError> {
        let mut collections = self.collections.write().await;
        let name = &request.name;
        let partition_count = request.partition_count;
        if collections.contains_key(name) {
            return Err(FlareError::InvalidArgument(
                "collection already exist".into(),
            ));
        }

        let mut shard_ids = Vec::with_capacity(partition_count as usize);
        let mut shards = self.shards.write().await;
        let mut shard_groups = Vec::with_capacity(partition_count as usize);

        let assignements = if request.shard_assignments.is_empty() {
            self.generate_default_assignments(&request, &mut shards)
        } else {
            request.shard_assignments
        };

        info!("create collection '{name}' with {partition_count} partitions: {assignements:?}");

        for partition_id in 0..partition_count {
            let assignment = assignements.get(partition_id as usize).ok_or(
                FlareError::InvalidArgument(
                    "invalid: shard assignments not match partition count"
                        .into(),
                ),
            )?;
            let mut replica_shard_ids = assignment.shard_ids.clone();
            let replica_owner_ids = assignment.replica.clone();
            if replica_shard_ids.is_empty() {
                for _ in 0..request.replica_count {
                    let id = shards.last_key_value().map(|e| *e.0).unwrap_or(1);
                    replica_shard_ids.push(id);
                }
            }
            for (i, shard_id) in replica_shard_ids.iter().enumerate() {
                let shard_meta = ShardMetadata {
                    id: *shard_id,
                    collection: name.into(),
                    partition_id: partition_id as u16,
                    owner: Some(replica_owner_ids[i % replica_owner_ids.len()]),
                    primary: assignment.primary,
                    replica: replica_shard_ids.clone(),
                    shard_type: request.shard_type.clone(),
                    ..Default::default()
                };
                info!("create shard {shard_meta:?}");
                shard_ids.push(*shard_id);
                shards.insert(*shard_id, shard_meta);
            }
            shard_groups.push(ShardGroupState {
                shard_ids: replica_shard_ids,
            });
        }

        let col_meta = CollectionMetadataState {
            name: name.into(),
            shards: shard_groups,
            replication: 1,
            ..Default::default()
        };
        collections.insert(name.into(), col_meta.clone());
        let num = *self.receiver.borrow();
        let _ = self.sender.send(num + 1).unwrap();
        info!("create collection '{name}'");
        Ok(CreateCollectionResponse {
            name: name.into(),
            ..Default::default()
        })
    }

    fn generate_default_assignments(
        &self,
        request: &CreateCollectionRequest,
        shards: &mut BTreeMap<u64, ShardMetadata>,
    ) -> Vec<ShardAssignment> {
        let partition_count = request.partition_count;
        let mut new_assignments = Vec::with_capacity(partition_count as usize);
        let mut shard_id_counter =
            shards.last_key_value().map(|e| *e.0).unwrap_or(0) + 1;
        for i in 0..partition_count {
            let mut shard_ids =
                Vec::with_capacity(request.replica_count as usize);
            let mut replica = vec![];
            let mut j = i as usize;
            while replica.len() < request.replica_count as usize {
                replica.push(self.members[j % self.members.len()]);
                j += 1;
            }
            let primary = replica[0];
            for _r in 0..request.replica_count {
                shard_ids.push(shard_id_counter);
                shard_id_counter += 1;
            }
            let assignment = ShardAssignment {
                replica: replica.clone(),
                primary: Some(primary),
                shard_ids,
            };
            new_assignments.push(assignment);
        }
        new_assignments
    }

    pub fn create_watch(&self) -> Receiver<u64> {
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

fn resolve_shard_id<'a>(
    meta: &'a CollectionMetadataState,
    key: &[u8],
) -> &'a ShardGroupState {
    let hashed = mur3::murmurhash3_x86_32(key, meta.seed) as u32;
    let shard_count = meta.shards.len();
    let size = u32::div_ceil(u32::MAX, shard_count as u32);
    let partition_index = hashed / size;
    return &meta.shards[partition_index as usize];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_default_assignments() {
        let manager = OprcMetaManager::new(1, vec![1, 2, 3]);
        let mut shards = BTreeMap::new();

        let request = CreateCollectionRequest {
            name: "test_collection".into(),
            partition_count: 3,
            replica_count: 2,
            ..Default::default()
        };

        let assignments =
            manager.generate_default_assignments(&request, &mut shards);

        assert_eq!(assignments.len(), 3); // 3 partitions

        for assignment in assignments.iter() {
            assert_eq!(
                assignment.replica.len(),
                request.replica_count as usize
            ); // 2 replicas per partition
            assert_eq!(
                assignment.shard_ids.len(),
                request.replica_count as usize
            ); // 2 shard ids per partition
            assert!(assignment.primary.is_some());
            assert!(assignment.replica.contains(&assignment.primary.unwrap()));
        }

        println!("assignments {:?}", assignments);

        // Check shard IDs are sequential
        let mut all_shard_ids: Vec<u64> = assignments
            .iter()
            .flat_map(|a| a.shard_ids.clone())
            .collect();
        all_shard_ids.sort();
        for i in 1..all_shard_ids.len() {
            assert_eq!(all_shard_ids[i], all_shard_ids[i - 1] + 1);
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct ShardGroupState {
    pub shard_ids: Vec<u64>,
}

#[derive(Debug, Default, Clone)]
pub struct CollectionMetadataState {
    pub name: String,
    pub shards: Vec<ShardGroupState>,
    pub seed: u32,
    pub replication: u8,
}
