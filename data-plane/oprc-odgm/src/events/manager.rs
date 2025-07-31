use std::sync::Arc;
use tracing::{debug, error};
use crate::shard::{ShardState, ObjectEntry};
use crate::events::types::{EventContext, EventType, TriggerExecutionContext};
use crate::events::processor::TriggerProcessor;
use oprc_pb::{ObjectEvent, TriggerTarget};

pub struct EventManager {
    trigger_processor: Arc<TriggerProcessor>,
}

impl EventManager {
    pub fn new(trigger_processor: Arc<TriggerProcessor>) -> Self {
        Self {
            trigger_processor,
        }
    }

    pub async fn trigger_event<S>(&self, context: EventContext, shard_state: &S) 
    where
        S: ShardState<Key = u64, Entry = ObjectEntry> + ?Sized,
    {
        // Fetch the object entry from shard state to get its events
        match shard_state.get(&context.object_id).await {
            Ok(Some(object_entry)) => {
                self.trigger_event_with_entry(context, &object_entry).await;
            }
            Ok(None) => {
                debug!("Object {} not found in shard", context.object_id);
            }
            Err(e) => {
                error!("Failed to fetch object {} from shard: {:?}", context.object_id, e);
            }
        }
    }

    /// Trigger event with object entry already available (more efficient)
    pub async fn trigger_event_with_entry(&self, context: EventContext, object_entry: &ObjectEntry) {
        if let Some(object_event) = &object_entry.event {
            let triggers = self.collect_matching_triggers(object_event, &context.event_type);
            
            for target in triggers {
                let exec_context = TriggerExecutionContext {
                    source_event: context.clone(),
                    target,
                };
                self.trigger_processor.execute_trigger(exec_context).await;
            }
        } else {
            debug!("No events configured for object {}", context.object_id);
        }
    }

    fn collect_matching_triggers(
        &self,
        object_event: &ObjectEvent,
        event_type: &EventType,
    ) -> Vec<TriggerTarget> {
        match event_type {
            EventType::FunctionComplete(fn_id) => {
                object_event.func_trigger.get(fn_id)
                    .map(|func_trigger| func_trigger.on_complete.clone())
                    .unwrap_or_default()
            }
            EventType::FunctionError(fn_id) => {
                object_event.func_trigger.get(fn_id)
                    .map(|func_trigger| func_trigger.on_error.clone())
                    .unwrap_or_default()
            }
            EventType::DataCreate(field_id) => {
                object_event.data_trigger.get(field_id)
                    .map(|data_trigger| data_trigger.on_create.clone())
                    .unwrap_or_default()
            }
            EventType::DataUpdate(field_id) => {
                object_event.data_trigger.get(field_id)
                    .map(|data_trigger| data_trigger.on_update.clone())
                    .unwrap_or_default()
            }
            EventType::DataDelete(field_id) => {
                object_event.data_trigger.get(field_id)
                    .map(|data_trigger| data_trigger.on_delete.clone())
                    .unwrap_or_default()
            }
        }
    }
}
