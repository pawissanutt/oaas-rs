use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ShardCapabilities {
    pub class: String,
    pub partition: u32,
    pub shard_id: u64,
    pub event_pipeline_v2: bool,
    pub storage_backend: String,
    pub odgm_version: String,
    pub features: Features,
}

#[derive(Debug, Clone, Serialize)]
pub struct Features {
    pub granular_storage: bool,
    pub bridge_mode: bool,
}

#[derive(Debug)]
pub enum CapErr {
    NotFound,
    BadRequest,
    Internal(String),
}
