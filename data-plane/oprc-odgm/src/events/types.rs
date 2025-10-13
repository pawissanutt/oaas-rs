use oprc_grpc::{TriggerPayload, TriggerTarget};
use std::collections::BTreeMap;

/// Internal event type enum for matching against protobuf EventType
#[derive(Debug, Clone)]
pub enum EventType {
    FunctionComplete(String), // function_id
    FunctionError(String),    // function_id
    DataCreate(u32),          // field_id
    DataUpdate(u32),          // field_id
    DataDelete(u32),          // field_id
}

#[derive(Debug, Clone)]
pub struct EventContext {
    pub object_id: u64,
    pub class_id: String,
    pub partition_id: u16,
    pub event_type: EventType,
    pub payload: Option<Vec<u8>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TriggerExecutionContext {
    pub source_event: EventContext,
    pub target: TriggerTarget,
}

#[derive(Debug, Clone, Copy)]
pub enum SerializationFormat {
    Json,
    Protobuf,
}

impl Default for SerializationFormat {
    fn default() -> Self {
        Self::Json
    }
}

/// Helper functions for creating and serializing trigger payloads
pub fn create_trigger_payload(
    context: &TriggerExecutionContext,
) -> TriggerPayload {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Map internal EventType to protobuf EventType enum
    let event_type = match &context.source_event.event_type {
        EventType::FunctionComplete(_) => {
            oprc_grpc::EventType::FuncComplete as i32
        }
        EventType::FunctionError(_) => oprc_grpc::EventType::FuncError as i32,
        EventType::DataCreate(_) => oprc_grpc::EventType::DataCreate as i32,
        EventType::DataUpdate(_) => oprc_grpc::EventType::DataUpdate as i32,
        EventType::DataDelete(_) => oprc_grpc::EventType::DataDelete as i32,
    };

    // Extract function ID or key based on event type
    let (fn_id, key) = match &context.source_event.event_type {
        EventType::FunctionComplete(fn_id)
        | EventType::FunctionError(fn_id) => (Some(fn_id.clone()), None),
        EventType::DataCreate(key)
        | EventType::DataUpdate(key)
        | EventType::DataDelete(key) => (None, Some(*key)),
    };

    // Create EventInfo from protobuf
    let event_info = oprc_grpc::EventInfo {
        source_cls_id: context.source_event.class_id.clone(),
        source_partition_id: context.source_event.partition_id as u32,
        source_object_id: context.source_event.object_id,
        event_type,
        fn_id,
        key,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        context: BTreeMap::new(), // Can be extended with additional metadata
    };

    TriggerPayload {
        event_info: Some(event_info),
        original_payload: context.source_event.payload.clone(),
    }
}

/// Serialize to JSON (default format)
pub fn serialize_trigger_payload_json(
    payload: &TriggerPayload,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(payload)
}

/// Serialize to protobuf (when protobuf support is needed)
pub fn serialize_trigger_payload_protobuf(payload: &TriggerPayload) -> Vec<u8> {
    use prost::Message;
    payload.encode_to_vec()
}

/// Get serialized payload in the specified format
pub fn serialize_trigger_payload(
    payload: &TriggerPayload,
    format: SerializationFormat,
) -> Vec<u8> {
    match format {
        SerializationFormat::Json => {
            serialize_trigger_payload_json(payload).unwrap_or_default()
        }
        SerializationFormat::Protobuf => {
            serialize_trigger_payload_protobuf(payload)
        }
    }
}
