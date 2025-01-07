use super::ObjectEntry;

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
