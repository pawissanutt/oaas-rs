pub mod config;
pub mod manager;
pub mod processor;
pub mod types;

pub use config::EventConfig;
pub use manager::EventManagerImpl;
pub use processor::TriggerProcessor;
pub use types::*;

use crate::shard::ObjectEntry;

#[async_trait::async_trait]
pub trait EventManager {
    async fn trigger_event(&self, context: EventContext);
    async fn trigger_event_with_entry(
        &self,
        context: EventContext,
        object_entry: &ObjectEntry,
    );
}
