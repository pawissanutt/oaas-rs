use crate::events::types::SerializationFormat;

#[derive(Debug, Clone)]
pub struct EventConfig {
    pub max_trigger_depth: u32,  // Prevent infinite loops
    pub trigger_timeout_ms: u64, // Timeout for trigger execution
    pub enable_event_logging: bool, // Log all event executions
    pub max_concurrent_triggers: u32, // Limit concurrent trigger executions
    pub payload_format: SerializationFormat, // Default serialization format
}

impl Default for EventConfig {
    fn default() -> Self {
        Self {
            max_trigger_depth: 10,
            trigger_timeout_ms: 30000,
            enable_event_logging: true,
            max_concurrent_triggers: 100,
            payload_format: SerializationFormat::Json, // JSON by default
        }
    }
}
