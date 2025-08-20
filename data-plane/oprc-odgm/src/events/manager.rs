use crate::events::EventManager;
use crate::events::processor::TriggerProcessor;
use crate::events::types::{EventContext, EventType, TriggerExecutionContext};
use crate::shard::ObjectEntry;
use oprc_dp_storage::{ApplicationDataStorage, StorageValue};
use oprc_pb::{ObjectEvent, TriggerTarget};
use std::sync::Arc;
use tracing::{debug, warn};

pub struct EventManagerImpl<S: ApplicationDataStorage> {
    trigger_processor: Arc<TriggerProcessor>,
    app_storage: S,
}

impl<S: ApplicationDataStorage + 'static> EventManagerImpl<S> {
    pub fn new(
        trigger_processor: Arc<TriggerProcessor>,
        app_storage: S,
    ) -> Self {
        Self {
            trigger_processor,
            app_storage,
        }
    }

    /// Trigger event by loading object from storage
    /// This is the main method that should be called from InvocationOffloader
    pub async fn trigger_event(&self, context: EventContext) {
        let object_id = context.object_id;
        let key = object_id.to_be_bytes();

        match self.app_storage.get(&key).await {
            Ok(Some(storage_value)) => {
                match self.deserialize_object_entry(&storage_value) {
                    Ok(object_entry) => {
                        self.trigger_event_with_entry(context, &object_entry)
                            .await;
                    }
                    Err(e) => {
                        warn!(
                            "Failed to deserialize object entry for object {}: {}",
                            object_id, e
                        );
                    }
                }
            }
            Ok(None) => {
                debug!(
                    "Object {} not found in storage for event triggering",
                    object_id
                );
            }
            Err(e) => {
                warn!(
                    "Failed to load object {} from storage for event triggering: {}",
                    object_id, e
                );
            }
        }
    }

    /// Trigger event with object entry already available (more efficient)
    pub async fn trigger_event_with_entry(
        &self,
        context: EventContext,
        object_entry: &ObjectEntry,
    ) {
        if let Some(object_event) = &object_entry.event {
            let triggers = self
                .collect_matching_triggers(object_event, &context.event_type);

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
            EventType::FunctionComplete(fn_id) => object_event
                .func_trigger
                .get(fn_id)
                .map(|func_trigger| func_trigger.on_complete.clone())
                .unwrap_or_default(),
            EventType::FunctionError(fn_id) => object_event
                .func_trigger
                .get(fn_id)
                .map(|func_trigger| func_trigger.on_error.clone())
                .unwrap_or_default(),
            EventType::DataCreate(field_id) => object_event
                .data_trigger
                .get(field_id)
                .map(|data_trigger| data_trigger.on_create.clone())
                .unwrap_or_default(),
            EventType::DataUpdate(field_id) => object_event
                .data_trigger
                .get(field_id)
                .map(|data_trigger| data_trigger.on_update.clone())
                .unwrap_or_default(),
            EventType::DataDelete(field_id) => object_event
                .data_trigger
                .get(field_id)
                .map(|data_trigger| data_trigger.on_delete.clone())
                .unwrap_or_default(),
        }
    }

    /// Deserialize StorageValue to ObjectEntry
    fn deserialize_object_entry(
        &self,
        storage_value: &StorageValue,
    ) -> Result<ObjectEntry, Box<dyn std::error::Error + Send + Sync>> {
        let bytes = storage_value.as_slice();
        match bincode::serde::decode_from_slice(
            bytes,
            bincode::config::standard(),
        ) {
            Ok((entry, _)) => Ok(entry), // bincode v2 returns (T, bytes_read)
            Err(e) => Err(Box::new(e)),
        }
    }
}

// Implement the EventManager trait for EventManagerImpl
#[async_trait::async_trait]
impl<S: ApplicationDataStorage + 'static> EventManager for EventManagerImpl<S> {
    async fn trigger_event(&self, context: EventContext) {
        self.trigger_event(context).await
    }

    async fn trigger_event_with_entry(
        &self,
        context: EventContext,
        object_entry: &ObjectEntry,
    ) {
        self.trigger_event_with_entry(context, object_entry).await
    }
}
