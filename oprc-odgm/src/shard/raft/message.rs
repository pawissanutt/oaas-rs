use oprc_pb::{SetObjectRequest, SingleObjectRequest};
use std::io::Cursor;

use crate::shard::ObjectEntry;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ShardReq {
    Set(SetObjectRequest),
    // SetValua(SetKeyRequest),
    // Merge(SetObjectRequest),
    Delete(SingleObjectRequest),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub enum ShardResp {
    Empty,
    None,
    Item(ObjectEntry),
}
