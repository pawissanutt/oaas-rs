use std::collections::BTreeMap;

use crate::shard::ObjectEntry;

use super::generic::AppStateMachine;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum ShardReq {
    Get(u64),
    Set(u64, ObjectEntry),
    Delete(u64),
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum ShardResp {
    Empty,
    None,
    Item(ObjectEntry),
}

const BINCODE_CONFIG: bincode::config::Configuration =
    bincode::config::standard();

#[derive(Default, Clone)]
pub struct ObjectShardStateMachine {
    data: BTreeMap<u64, ObjectEntry>,
}

impl AppStateMachine for ObjectShardStateMachine {
    type Req = ShardReq;

    type Resp = ShardResp;

    fn load(data: &[u8]) -> Result<Self, openraft::AnyError> {
        let resp = bincode::serde::decode_from_slice(data, BINCODE_CONFIG)
            .map_err(|e| openraft::AnyError::new(&e))?
            .0;
        Ok(ObjectShardStateMachine { data: resp })
    }

    fn to_vec(&self) -> Result<Vec<u8>, openraft::AnyError> {
        let encoded = bincode::serde::encode_to_vec(&self.data, BINCODE_CONFIG)
            .map_err(|e| openraft::AnyError::new(&e))?;
        Ok(encoded)
    }

    fn apply(&mut self, req: &Self::Req) -> Self::Resp {
        match req {
            ShardReq::Get(key) => {
                if let Some(entry) = self.data.get(key) {
                    ShardResp::Item(entry.clone())
                } else {
                    ShardResp::None
                }
            }
            ShardReq::Set(key, entry) => {
                self.data.insert(*key, entry.clone());
                ShardResp::Empty
            }
            ShardReq::Delete(key) => {
                self.data.remove(key);
                ShardResp::Empty
            }
        }
    }

    fn empty_resp(&self) -> Self::Resp {
        ShardResp::Empty
    }
}
