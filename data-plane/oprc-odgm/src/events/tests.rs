use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::error::OdgmError;
use crate::events::types::{
    create_trigger_payload, serialize_trigger_payload, EventContext, EventType,
    SerializationFormat, TriggerExecutionContext,
};
use crate::shard::{ObjectEntry, ShardMetadata, ShardState};
use oprc_pb::{
    DataTrigger, EventType as PbEventType, FuncTrigger, ObjectEvent,
    TriggerTarget,
};

// Mock implementations for testing

/// Mock shard state for testing event triggering
struct MockShardState {
    objects: Arc<Mutex<HashMap<u64, ObjectEntry>>>,
    metadata: ShardMetadata,
    _readiness_sender: tokio::sync::watch::Sender<bool>,
    readiness_receiver: tokio::sync::watch::Receiver<bool>,
}

impl MockShardState {
    fn new() -> Self {
        let metadata = ShardMetadata {
            id: 1,
            collection: "test_collection".to_string(),
            partition_id: 0,
            owner: Some(1),
            primary: Some(1),
            shard_type: "test".to_string(),
            ..Default::default()
        };
        let (readiness_sender, readiness_receiver) =
            tokio::sync::watch::channel(true);

        Self {
            objects: Arc::new(Mutex::new(HashMap::new())),
            metadata,
            _readiness_sender: readiness_sender,
            readiness_receiver,
        }
    }

    async fn insert(&self, id: u64, entry: ObjectEntry) {
        self.objects.lock().await.insert(id, entry);
    }
}

#[async_trait::async_trait]
impl ShardState for MockShardState {
    type Key = u64;
    type Entry = ObjectEntry;

    fn meta(&self) -> &ShardMetadata {
        &self.metadata
    }

    fn watch_readiness(&self) -> tokio::sync::watch::Receiver<bool> {
        self.readiness_receiver.clone()
    }

    async fn get(
        &self,
        key: &Self::Key,
    ) -> Result<Option<Self::Entry>, OdgmError> {
        Ok(self.objects.lock().await.get(key).cloned())
    }

    async fn set(
        &self,
        _key: Self::Key,
        _entry: Self::Entry,
    ) -> Result<(), OdgmError> {
        unimplemented!("set not needed for event tests")
    }

    async fn delete(&self, _key: &Self::Key) -> Result<(), OdgmError> {
        unimplemented!("delete not needed for event tests")
    }

    async fn count(&self) -> Result<u64, OdgmError> {
        Ok(self.objects.lock().await.len() as u64)
    }
}

// Helper functions for creating test data

fn create_test_object_entry_with_events() -> ObjectEntry {
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut object_event = ObjectEvent::default();

    // Add data triggers
    let mut data_trigger = DataTrigger::default();
    data_trigger.on_create.push(TriggerTarget {
        cls_id: "notification_service".to_string(),
        partition_id: 1,
        object_id: None,
        fn_id: "on_data_create".to_string(),
        req_options: HashMap::new(),
    });
    data_trigger.on_update.push(TriggerTarget {
        cls_id: "notification_service".to_string(),
        partition_id: 1,
        object_id: Some(42),
        fn_id: "on_data_update".to_string(),
        req_options: HashMap::new(),
    });
    data_trigger.on_delete.push(TriggerTarget {
        cls_id: "audit_service".to_string(),
        partition_id: 2,
        object_id: None,
        fn_id: "on_data_delete".to_string(),
        req_options: HashMap::new(),
    });

    object_event.data_trigger.insert(100, data_trigger);

    // Add function triggers
    let mut func_trigger = FuncTrigger::default();
    func_trigger.on_complete.push(TriggerTarget {
        cls_id: "completion_service".to_string(),
        partition_id: 0,
        object_id: None,
        fn_id: "on_function_complete".to_string(),
        req_options: HashMap::new(),
    });
    func_trigger.on_error.push(TriggerTarget {
        cls_id: "error_service".to_string(),
        partition_id: 0,
        object_id: None,
        fn_id: "on_function_error".to_string(),
        req_options: HashMap::new(),
    });

    object_event
        .func_trigger
        .insert("test_function".to_string(), func_trigger);

    ObjectEntry {
        last_updated: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64,
        value: BTreeMap::new(),
        event: Some(object_event),
    }
}

fn create_test_event_context(event_type: EventType) -> EventContext {
    EventContext {
        object_id: 12345,
        class_id: "test_class".to_string(),
        partition_id: 1,
        event_type,
        payload: Some(b"test payload".to_vec()),
        error_message: None,
    }
}

// Unit Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_payload_function_event() {
        let context = create_test_event_context(EventType::FunctionComplete(
            "my_func".to_string(),
        ));
        let target = TriggerTarget {
            cls_id: "completion_service".to_string(),
            partition_id: 0,
            object_id: None,
            fn_id: "on_complete".to_string(),
            req_options: HashMap::new(),
        };

        let exec_context = TriggerExecutionContext {
            source_event: context,
            target,
        };

        let payload = create_trigger_payload(&exec_context);
        let event_info = payload.event_info.unwrap();

        assert_eq!(event_info.event_type, PbEventType::FuncComplete as i32);
        assert_eq!(event_info.fn_id, Some("my_func".to_string()));
        assert!(event_info.key.is_none());
    }

    #[test]
    fn test_payload_serialization_json() {
        let context = create_test_event_context(EventType::DataUpdate(42));
        let target = TriggerTarget {
            cls_id: "test_service".to_string(),
            partition_id: 1,
            object_id: None,
            fn_id: "handle_update".to_string(),
            req_options: HashMap::new(),
        };

        let exec_context = TriggerExecutionContext {
            source_event: context,
            target,
        };

        let payload = create_trigger_payload(&exec_context);
        let serialized =
            serialize_trigger_payload(&payload, SerializationFormat::Json);

        assert!(!serialized.is_empty());

        // Verify it's valid JSON by deserializing
        let deserialized: serde_json::Value =
            serde_json::from_slice(&serialized).unwrap();
        assert!(deserialized.get("event_info").is_some());
    }

    #[test]
    fn test_payload_serialization_protobuf() {
        let context = create_test_event_context(EventType::DataDelete(99));
        let target = TriggerTarget {
            cls_id: "audit_service".to_string(),
            partition_id: 3,
            object_id: Some(123),
            fn_id: "audit_delete".to_string(),
            req_options: HashMap::new(),
        };

        let exec_context = TriggerExecutionContext {
            source_event: context,
            target,
        };

        let payload = create_trigger_payload(&exec_context);
        let serialized =
            serialize_trigger_payload(&payload, SerializationFormat::Protobuf);

        assert!(!serialized.is_empty());

        // Verify it can be decoded back
        use prost::Message;
        let decoded = oprc_pb::TriggerPayload::decode(&serialized[..]).unwrap();
        assert!(decoded.event_info.is_some());
    }

    #[tokio::test]
    async fn test_mock_shard_state() {
        let shard = MockShardState::new();
        let entry = create_test_object_entry_with_events();

        // Insert object
        shard.insert(123, entry.clone()).await;

        // Retrieve object
        let result = shard.get(&123).await.unwrap();
        assert!(result.is_some());

        let retrieved = result.unwrap();
        assert!(retrieved.event.is_some());

        // Test non-existent object
        let missing = shard.get(&999).await.unwrap();
        assert!(missing.is_none());
    }
}

// Integration Tests

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_event_manager_trigger_collection() {
        // Test the trigger collection logic directly without the full EventManager setup
        let entry = create_test_object_entry_with_events();

        // Test data create triggers
        if let Some(object_event) = &entry.event {
            let triggers = collect_matching_triggers_for_test(
                object_event,
                &EventType::DataCreate(100),
            );
            assert_eq!(triggers.len(), 1);
            assert_eq!(triggers[0].cls_id, "notification_service");
            assert_eq!(triggers[0].fn_id, "on_data_create");
        }
    }

    // Helper function extracted from EventManager for testing
    pub fn collect_matching_triggers_for_test(
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

    #[tokio::test]
    async fn test_data_create_trigger_collection() {
        let entry = create_test_object_entry_with_events();

        if let Some(object_event) = &entry.event {
            let triggers = collect_matching_triggers_for_test(
                object_event,
                &EventType::DataCreate(100),
            );
            assert_eq!(triggers.len(), 1);
            assert_eq!(triggers[0].cls_id, "notification_service");
            assert_eq!(triggers[0].fn_id, "on_data_create");
            assert_eq!(triggers[0].partition_id, 1);
            assert!(triggers[0].object_id.is_none());
        }
    }

    #[tokio::test]
    async fn test_data_update_trigger_collection() {
        let entry = create_test_object_entry_with_events();

        if let Some(object_event) = &entry.event {
            let triggers = collect_matching_triggers_for_test(
                object_event,
                &EventType::DataUpdate(100),
            );
            assert_eq!(triggers.len(), 1);
            assert_eq!(triggers[0].cls_id, "notification_service");
            assert_eq!(triggers[0].fn_id, "on_data_update");
            assert_eq!(triggers[0].object_id, Some(42));
        }
    }

    #[tokio::test]
    async fn test_function_complete_trigger_collection() {
        let entry = create_test_object_entry_with_events();

        if let Some(object_event) = &entry.event {
            let triggers = collect_matching_triggers_for_test(
                object_event,
                &EventType::FunctionComplete("test_function".to_string()),
            );
            assert_eq!(triggers.len(), 1);
            assert_eq!(triggers[0].cls_id, "completion_service");
            assert_eq!(triggers[0].fn_id, "on_function_complete");
        }
    }

    #[tokio::test]
    async fn test_no_triggers_configured() {
        let entry = create_test_object_entry_with_events();

        if let Some(object_event) = &entry.event {
            // Test event for field that has no triggers
            let triggers = collect_matching_triggers_for_test(
                object_event,
                &EventType::DataCreate(999),
            );
            assert_eq!(triggers.len(), 0);
        }
    }

    #[tokio::test]
    async fn test_multiple_triggers_same_event() {
        use std::time::{SystemTime, UNIX_EPOCH};

        // Create object with multiple triggers for the same event
        let mut object_event = ObjectEvent::default();
        let mut data_trigger = DataTrigger::default();

        // Add multiple targets for the same event type
        data_trigger.on_create.push(TriggerTarget {
            cls_id: "service_1".to_string(),
            partition_id: 1,
            object_id: None,
            fn_id: "handler_1".to_string(),
            req_options: HashMap::new(),
        });
        data_trigger.on_create.push(TriggerTarget {
            cls_id: "service_2".to_string(),
            partition_id: 2,
            object_id: Some(42),
            fn_id: "handler_2".to_string(),
            req_options: HashMap::new(),
        });

        object_event.data_trigger.insert(50, data_trigger);

        let entry = ObjectEntry {
            last_updated: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            value: BTreeMap::new(),
            event: Some(object_event),
        };

        if let Some(object_event) = &entry.event {
            let triggers = collect_matching_triggers_for_test(
                object_event,
                &EventType::DataCreate(50),
            );
            assert_eq!(triggers.len(), 2);

            let services: Vec<&str> = triggers
                .iter()
                .map(|trigger| trigger.cls_id.as_str())
                .collect();
            assert!(services.contains(&"service_1"));
            assert!(services.contains(&"service_2"));
        }
    }
}

// Performance tests have been migrated to Criterion benchmarks in `benches/event_bench.rs`.
// See that file for up-to-date performance benchmarks for event system logic.
