pub mod config;
pub mod manager;
pub mod processor;
pub mod types;
pub mod bridge; // J0 bridge summary emitter
pub mod v2; // J2 per-entry pipeline (skeleton)

pub use config::EventConfig;
pub use manager::EventManagerImpl;
pub use processor::TriggerProcessor;
pub use types::*;
pub use bridge::{BridgeDispatcher, BridgeDispatcherRef, BridgeSummaryEvent, BridgeConfig, build_bridge_event};
pub use v2::{V2Dispatcher, V2DispatcherRef};

// V2 mutation context scaffolding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutAction { Create, Update, Delete }

#[derive(Debug, Clone)]
pub struct ChangedKey { pub key_canonical: String, pub action: MutAction }

#[derive(Debug, Clone)]
pub struct MutationContext {
    pub object_id: String,
    pub cls_id: String,
    pub partition_id: u16,
    pub version_before: u64,
    pub version_after: u64,
    pub changed: Vec<ChangedKey>,
    pub event_config: Option<std::sync::Arc<oprc_grpc::ObjectEvent>>,
}

impl MutationContext {
    pub fn new(object_id: String, cls_id: String, partition_id: u16, version_before: u64, version_after: u64, changed: Vec<ChangedKey>) -> Self {
        Self { object_id, cls_id, partition_id, version_before, version_after, changed, event_config: None }
    }

    pub fn with_event_config(mut self, cfg: Option<std::sync::Arc<oprc_grpc::ObjectEvent>>) -> Self { self.event_config = cfg; self }
}

use crate::shard::ObjectData;

#[async_trait::async_trait]
pub trait EventManager {
    async fn trigger_event(&self, context: EventContext);
    async fn trigger_event_with_entry(
        &self,
        context: EventContext,
        object_entry: &ObjectData,
    );
}
