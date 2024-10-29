use std::sync::Arc;

use flare_dht::{
    error::FlareError,
    shard::{KvShard, ShardFactory, ShardMetadata},
};
use oprc_pb::{ObjData, ObjectReponse, ValData};

use scc::{
    hash_map::Entry::{Occupied, Vacant},
    HashMap,
};

use tracing::info;

pub struct ObjectShardFactory {}

impl ShardFactory<ObjectShard> for ObjectShardFactory {
    fn create_shard(
        &self,
        shard_metadata: ShardMetadata,
    ) -> std::sync::Arc<ObjectShard> {
        info!("create shard {:?}", &shard_metadata);
        let shard = ObjectShard {
            shard_metadata: shard_metadata,
            ..Default::default()
        };
        let shard = Arc::new(shard);
        shard
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct ObjectShard {
    pub shard_metadata: ShardMetadata,
    pub map: HashMap<u64, ObjectEntry>,
}

#[async_trait::async_trait]
impl KvShard for ObjectShard {
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
                f(entry)
            }
            Vacant(vacant_entry) => {
                let mut entry = Self::Entry::default();
                let o = f(&mut entry);
                vacant_entry.insert_entry(entry);
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
        self.map.upsert_async(key, value).await;
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), FlareError> {
        self.map.remove_async(key).await;
        Ok(())
    }
}

#[derive(Debug, Default, Clone)]
pub struct ObjectEntry {
    // pub rc: u16,
    // pub value: Vec<u8>,
    pub value: std::collections::HashMap<u32, ValData>, // pub value: Bytes,
}

// use prost::Message;

// impl ShardEntry for ObjectEntry {
//     fn to_vec(&self) -> Vec<u8> {
//         self.to_data().encode_to_vec()
//     }

//     fn from_vec(v: Vec<u8>) -> Self {
//         let data = ObjData::decode(&v[..]).unwrap();
//         ObjectEntry::from_data(data)
//     }
// }

impl ObjectEntry {
    pub fn merge(&self, other: Self) -> Self {
        todo!()
    }

    #[inline]
    pub fn to_resp(&self) -> ObjectReponse {
        ObjectReponse {
            obj: Some(self.to_data()),
        }
    }

    #[inline]
    pub fn to_data(&self) -> ObjData {
        ObjData {
            entries: self.value.clone(),
        }
    }

    #[inline]
    pub fn from_data(data: ObjData) -> Self {
        Self {
            value: data.entries,
        }
    }
}
