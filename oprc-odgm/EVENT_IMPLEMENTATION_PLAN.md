# Event Triggering System Implementation Plan

This document outlines the implementation plan for the ObjectEvent triggering system in OPRC-ODGM, as described in the API reference.

## Key Design Decisions

**Pure Asynchronous Trigger Execution**: The system uses Zenoh's async invocation API for trigger execution, providing:
- **True fire-and-forget semantics**: Triggers are dispatched without any result tracking
- **Maximum performance**: Zero blocking, zero waiting, zero result handling overhead
- **Fault tolerance**: Trigger failures are completely isolated from the original operation
- **Scalability**: Unlimited concurrent trigger dispatching through Zenoh's async messaging

**Zenoh Integration**: All trigger execution goes through Zenoh PUT operations to async invocation endpoints, enabling:
- Consistent invocation patterns with the existing system
- Natural load balancing and distribution across nodes
- Zero-overhead event processing pipeline

**Protobuf-Generated Types**: The system uses protobuf-generated types for TriggerPayload and EventInfo:
- **Structured Data**: Event information is well-defined and strongly typed
- **Dual Serialization**: Supports both JSON and protobuf serialization with the same types
- **Serde Integration**: Automatic JSON serialization/deserialization via serde
- **Simplified Design**: Removed TriggerInfo as target information is implicit in the invocation context

## Overview

The event triggering system allows objects to define triggers that automatically execute functions based on:
1. **Function completion/error events** - Execute actions when methods complete or fail
2. **Data change events** - Execute actions when object data is created, updated, or deleted

## Implementation Status

**Completed:**
- ✅ Protobuf definitions for TriggerPayload and EventInfo in `oprc-pb/proto/oprc-data.proto`
- ✅ Simplified design with TriggerInfo removed (target info implicit in invocation context)
- ✅ Dual serialization support (JSON/protobuf) with same generated types
- ✅ Serde integration for automatic JSON handling
- ✅ Comprehensive implementation plan with all integration points defined
- ✅ Unified configuration approach using single EventConfig struct

**Next Steps:**
1. Build oprc-pb to regenerate Rust types from updated protobuf definitions
2. Create the event system modules (`events/types.rs`, `events/manager.rs`, etc.)
3. Implement and test the event manager and trigger processor
4. Integrate with existing shard and invocation systems
5. Add comprehensive unit and integration tests

## Architecture

### Components

1. **Event Manager** - Core component managing event registration and dispatch
2. **Trigger Processor** - Executes trigger targets when events occur
3. **Event Storage** - Persistent storage for object events
4. **Integration Points** - Hooks into existing invocation and data systems

### Event Flow

```
Object Operation → Event Detection → Trigger Evaluation → Target Execution
```

## Implementation Plan

### Phase 1: Core Event Infrastructure

#### 1.1 Define Event Types and Structures

**File:** `oprc-odgm/src/events/types.rs`

```rust
use std::collections::HashMap;
use oprc_pb::{ObjectEvent, FuncTrigger, DataTrigger, TriggerTarget, TriggerPayload, EventInfo};
use serde::{Deserialize, Serialize};

/// Internal event type enum for matching against protobuf EventType
#[derive(Debug, Clone)]
pub enum EventType {
    FunctionComplete(String),  // function_id
    FunctionError(String),     // function_id
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

/// Extension trait for TriggerPayload to add convenience methods
impl TriggerPayload {
    pub fn new(context: &TriggerExecutionContext) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        
        // Map internal EventType to protobuf EventType enum
        let event_type = match &context.source_event.event_type {
            EventType::FunctionComplete(_) => oprc_pb::EventType::EventTypeFuncComplete as i32,
            EventType::FunctionError(_) => oprc_pb::EventType::EventTypeFuncError as i32,
            EventType::DataCreate(_) => oprc_pb::EventType::EventTypeDataCreate as i32,
            EventType::DataUpdate(_) => oprc_pb::EventType::EventTypeDataUpdate as i32,
            EventType::DataDelete(_) => oprc_pb::EventType::EventTypeDataDelete as i32,
        };

        // Extract function ID or key based on event type
        let (fn_id, key) = match &context.source_event.event_type {
            EventType::FunctionComplete(fn_id) | EventType::FunctionError(fn_id) => {
                (Some(fn_id.clone()), None)
            }
            EventType::DataCreate(key) | EventType::DataUpdate(key) | EventType::DataDelete(key) => {
                (None, Some(*key))
            }
        };

        // Create EventInfo from protobuf
        let event_info = oprc_pb::EventInfo {
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
            context: std::collections::HashMap::new(), // Can be extended with additional metadata
        };

        Self {
            event_info: Some(event_info),
            original_payload: context.source_event.payload.clone(),
        }
    }

    /// Serialize to JSON (default format)
    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Serialize to protobuf (when protobuf support is needed)
    pub fn to_protobuf(&self) -> Vec<u8> {
        use prost::Message;
        self.encode_to_vec()
    }

    /// Get serialized payload in the specified format
    pub fn serialize(&self, format: SerializationFormat) -> Vec<u8> {
        match format {
            SerializationFormat::Json => self.to_json().unwrap_or_default(),
            SerializationFormat::Protobuf => self.to_protobuf(),
        }
    }
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
```

#### 1.2 Event Manager Core

**File:** `oprc-odgm/src/events/manager.rs`

```rust
use std::sync::Arc;
use tracing::{debug, error, info};
use crate::shard::ObjectShardState;
use crate::events::types::{EventContext, EventType, TriggerExecutionContext};
use oprc_pb::{ObjectEvent, TriggerTarget, ObjectEntry};

pub struct EventManager {
    trigger_processor: Arc<TriggerProcessor>,
    shard_state: ObjectShardState,
}

impl EventManager {
    pub fn new(trigger_processor: Arc<TriggerProcessor>, shard_state: ObjectShardState) -> Self {
        Self {
            trigger_processor,
            shard_state,
        }
    }

    pub async fn trigger_event(&self, context: EventContext) {
        // Fetch the object entry from shard state to get its events
        match self.shard_state.get(&context.object_id).await {
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
        if let Some(object_event) = &object_entry.events {
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
```

#### 1.3 Trigger Processor

**File:** `oprc-odgm/src/events/processor.rs`

```rust
use std::sync::Arc;
use oprc_pb::{InvocationRequest, ObjectInvocationRequest, TriggerPayload};
use prost::Message;
use tracing::{debug, error, warn};
use uuid::Uuid;
use zenoh::Session;
use zenoh::bytes::ZBytes;
use crate::events::types::{TriggerExecutionContext, SerializationFormat};
use crate::events::config::EventConfig;

pub struct TriggerProcessor {
    // Zenoh session for async invocation
    z_session: Session,
    // Configuration for trigger execution
    config: EventConfig,
}

impl TriggerProcessor {
    pub fn new(z_session: Session, config: EventConfig) -> Self {
        Self {
            z_session,
            config,
        }
    }

    /// Execute a trigger using Zenoh's fire-and-forget async invocation
    pub async fn execute_trigger(&self, context: TriggerExecutionContext) {
        debug!(
            "Executing trigger via Zenoh async invocation: {:?} -> {:?}",
            context.source_event.event_type,
            context.target
        );

        // Prepare trigger payload with event context
        let trigger_payload = self.prepare_trigger_payload(&context);

        // Generate unique invocation ID for tracking
        let invocation_id = Uuid::new_v4().to_string();

        // Execute fire-and-forget async invocation via Zenoh PUT
        match self.execute_async_invocation(context, trigger_payload, &invocation_id).await {
            Ok(_) => {
                debug!("Trigger dispatched successfully: invocation_id={}", invocation_id);
            }
            Err(e) => {
                error!("Failed to dispatch trigger: invocation_id={}, error={:?}", invocation_id, e);
            }
        }
    }

    async fn execute_async_invocation(
        &self,
        context: TriggerExecutionContext,
        payload: Vec<u8>,
        invocation_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Construct Zenoh key expression for async invocation
        let key_expr = self.build_async_key_expr(&context, invocation_id);

        // Build the appropriate invocation request
        let encoded_payload = self.build_invocation_request(&context, payload)?;

        // Fire-and-forget PUT via Zenoh (async invocation)
        // This uses Zenoh's async invocation pattern where:
        // 1. PUT operation dispatches the invocation request
        // 2. The invocation is handled asynchronously by the target partition
        // 3. No response is expected (true fire-and-forget)
        // 4. Results are ignored for maximum performance
        self.z_session
            .put(key_expr, ZBytes::from(encoded_payload))
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        Ok(())
    }

    fn build_async_key_expr(&self, context: &TriggerExecutionContext, invocation_id: &str) -> String {
        match context.target.object_id {
            Some(target_object_id) => {
                // Async object method invocation key
                format!(
                    "oprc/{}/{}/objects/{}/async/{}/{}",
                    context.target.cls_id,
                    context.target.partition_id,
                    target_object_id,
                    context.target.fn_id,
                    invocation_id
                )
            }
            None => {
                // Async stateless function invocation key
                format!(
                    "oprc/{}/{}/async/{}/{}",
                    context.target.cls_id,
                    context.target.partition_id,
                    context.target.fn_id,
                    invocation_id
                )
            }
        }
    }

    fn build_invocation_request(
        &self,
        context: &TriggerExecutionContext,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        match context.target.object_id {
            Some(target_object_id) => {
                let request = ObjectInvocationRequest {
                    partition_id: context.target.partition_id,
                    object_id: target_object_id,
                    cls_id: context.target.cls_id.clone(),
                    fn_id: context.target.fn_id.clone(),
                    options: context.target.req_options.clone(),
                    payload,
                };
                Ok(request.encode_to_vec())
            }
            None => {
                let request = InvocationRequest {
                    partition_id: context.target.partition_id,
                    cls_id: context.target.cls_id.clone(),
                    fn_id: context.target.fn_id.clone(),
                    options: context.target.req_options.clone(),
                    payload,
                };
                Ok(request.encode_to_vec())
            }
        }
    }

    fn prepare_trigger_payload(&self, context: &TriggerExecutionContext) -> Vec<u8> {
        // Create structured payload using the protobuf-generated TriggerPayload
        // Note: Target information (class, partition, function) is implicit in the
        // invocation context, so we only need to send event information
        let payload = TriggerPayload::new(context);
        
        // Serialize using the configured format (JSON by default)
        payload.serialize(self.config.payload_format)
    }
}
```

### Phase 2: Integration with Existing Systems

#### 2.1 Shard Integration

**File:** `oprc-odgm/src/shard/mod.rs` (modifications)

```rust
use std::sync::Arc;
use crate::events::manager::EventManager;
use crate::events::types::EventContext;
use oprc_pb::ObjectEntry;

// Add to ObjectShard struct
pub struct ObjectShard {
    // ...existing fields...
    event_manager: Option<Arc<EventManager>>,
}

impl ObjectShard {
    // Modify constructor to include event manager
    fn new(
        shard_state: Arc<dyn ShardState<Key = u64, Entry = ObjectEntry>>,
        z_session: zenoh::Session,
        event_manager: Option<Arc<EventManager>>,
    ) -> Self {
        // ...existing initialization...
        Self {
            // ...existing fields...
            event_manager,
        }
    }

    // Add method to trigger events
    async fn trigger_event(&self, context: EventContext) {
        if let Some(event_manager) = &self.event_manager {
            event_manager.trigger_event(context).await;
        }
    }

    // More efficient method when object entry is already available
    async fn trigger_event_with_entry(&self, context: EventContext, object_entry: &ObjectEntry) {
        if let Some(event_manager) = &self.event_manager {
            event_manager.trigger_event_with_entry(context, object_entry).await;
        }
    }
}
```

#### 2.2 Object Data Integration

**File:** `oprc-odgm/src/shard/basic.rs` (modifications)

```rust
use crate::events::types::{EventContext, EventType};
use crate::error::OdgmError;
use oprc_pb::ObjectEntry;

impl ShardState for BasicObjectShard {
    async fn set(
        &self,
        key: Self::Key,
        value: Self::Entry,
    ) -> Result<(), OdgmError> {
        let is_new = !self.map.contains_async(&key).await;
        let old_entry = if !is_new {
            self.map.get_async(&key).await.map(|e| e.clone())
        } else {
            None
        };

        self.map.upsert_async(key, value.clone()).await;

        // Trigger data events
        self.trigger_data_events(key, &value, old_entry.as_ref(), is_new).await;
        
        Ok(())
    }

    async fn delete(&self, key: &Self::Key) -> Result<(), OdgmError> {
        if let Some(deleted_entry) = self.map.remove_async(key).await {
            // Trigger delete events for all fields
            self.trigger_delete_events(*key, &deleted_entry).await;
        }
        Ok(())
    }
}

impl BasicObjectShard {
    async fn trigger_data_events(
        &self,
        object_id: u64,
        new_entry: &ObjectEntry,
        old_entry: Option<&ObjectEntry>,
        is_new: bool,
    ) {
        // Only trigger events if the new entry has events configured
        if new_entry.events.is_some() {
            let meta = &self.shard_metadata;
            
            // Compare fields and trigger appropriate events
            for (field_id, new_val) in &new_entry.entries {
                let event_type = if is_new {
                    EventType::DataCreate(*field_id)
                } else if old_entry
                    .and_then(|e| e.entries.get(field_id))
                    .map(|old_val| old_val != new_val)
                    .unwrap_or(true)
                {
                    EventType::DataUpdate(*field_id)
                } else {
                    continue; // No change
                };

                let context = EventContext {
                    object_id,
                    class_id: meta.collection.clone(),
                    partition_id: meta.partition_id,
                    event_type,
                    payload: Some(new_val.data.clone().unwrap_or_default()),
                    error_message: None,
                };

                // Trigger event through the shard's event manager (efficient version)
                self.trigger_event_with_entry(context, new_entry).await;
            }
        }
    }

    async fn trigger_delete_events(&self, object_id: u64, deleted_entry: &ObjectEntry) {
        // Only trigger events if the deleted entry had events configured
        if deleted_entry.events.is_some() {
            let meta = &self.shard_metadata;
            
            for field_id in deleted_entry.entries.keys() {
                let context = EventContext {
                    object_id,
                    class_id: meta.collection.clone(),
                    partition_id: meta.partition_id,
                    event_type: EventType::DataDelete(*field_id),
                    payload: None,
                    error_message: None,
                };

                // Trigger event through the shard's event manager (efficient version)
                self.trigger_event_with_entry(context, deleted_entry).await;
            }
        }
    }
}
```

#### 2.3 Function Invocation Integration

**File:** `oprc-odgm/src/shard/invocation/exec.rs` (modifications)

```rust
use std::sync::Arc;
use crate::events::manager::EventManager;
use crate::events::types::{EventContext, EventType};
use oprc_pb::{InvocationRequest, ObjectInvocationRequest, InvocationResponse};
use crate::error::OffloadError;

impl InvocationOffloader {
    pub async fn invoke_fn_with_events(
        &self,
        req: InvocationRequest,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<InvocationResponse, OffloadError> {
        let result = self.invoke_fn(req.clone()).await;

        if let Some(manager) = event_manager {
            let context = EventContext {
                object_id: 0, // Stateless functions don't have object_id
                class_id: req.cls_id.clone(),
                partition_id: req.partition_id as u16,
                event_type: match &result {
                    Ok(_) => EventType::FunctionComplete(req.fn_id.clone()),
                    Err(_) => EventType::FunctionError(req.fn_id.clone()),
                },
                payload: req.payload.into(),
                error_message: result.as_ref().err().map(|e| e.to_string()),
            };

            manager.trigger_event(context).await;
        }

        result
    }

    pub async fn invoke_obj_with_events(
        &self,
        req: ObjectInvocationRequest,
        event_manager: Option<Arc<EventManager>>,
    ) -> Result<InvocationResponse, OffloadError> {
        let result = self.invoke_obj(req.clone()).await;

        if let Some(manager) = event_manager {
            let context = EventContext {
                object_id: req.object_id,
                class_id: req.cls_id.clone(),
                partition_id: req.partition_id as u16,
                event_type: match &result {
                    Ok(_) => EventType::FunctionComplete(req.fn_id.clone()),
                    Err(_) => EventType::FunctionError(req.fn_id.clone()),
                },
                payload: req.payload.into(),
                error_message: result.as_ref().err().map(|e| e.to_string()),
            };

            manager.trigger_event(context).await;
        }

        result
    }
}
```

### Phase 3: Storage and Persistence

#### 3.1 Event Storage in Object Data

**File:** `oprc-odgm/src/shard/basic.rs` (ObjectEntry modifications)

**Required Changes:**
1. Add `events: Option<ObjectEvent>` field to ObjectEntry if not already present
2. Update From/Into conversions to handle events from ObjData

```rust
// Modify ObjectEntry to include events field
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Hash)]
pub struct ObjectEntry {
    pub entries: HashMap<u32, ObjectVal>,
    pub events: Option<ObjectEvent>, // ADD THIS FIELD if not already present
}

impl From<ObjData> for ObjectEntry {
    fn from(value: ObjData) -> Self {
        let mut entries = HashMap::new();
        for (k, v) in value.entries.iter() {
            entries.insert(*k, ObjectVal::from(v));
        }
        Self {
            entries,
            events: value.event, // ADD THIS LINE to store events from ObjData
        }
    }
}

impl Into<ObjData> for ObjectEntry {
    fn into(self) -> ObjData {
        let mut entries = HashMap::new();
        for (k, v) in self.entries.iter() {
            entries.insert(*k, v.into());
        }
        ObjData {
            metadata: None,
            entries,
            event: self.events, // ADD THIS LINE to include events in ObjData
        }
    }
}
```

#### 3.2 Event Integration with Existing API

**File:** `oprc-odgm/src/grpc_service/data.rs` (no changes needed)

**Note:** Once ObjectEntry includes the `events` field, the existing `set_object` method will automatically handle events since they are stored as part of the ObjectEntry. No API changes are required.

```rust
// The existing set_object method already works with events
// once ObjectEntry.events field is added
impl DataService for OdgmDataService {
    async fn set_object(
        &self,
        request: tonic::Request<ObjData>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status> {
        let obj_data = request.into_inner();
        let meta = obj_data.metadata.as_ref().ok_or_else(|| {
            Status::invalid_argument("metadata is required")
        })?;

        let shard = self
            .odgm
            .get_local_shard(&meta.cls_id, meta.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("shard not found"))?;

        // Convert and store object (events automatically included via From<ObjData>)
        let object_entry = ObjectEntry::from(obj_data);
        shard
            .set(meta.object_id, object_entry)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(EmptyResponse::default()))
    }
}
```

### Phase 4: Configuration and Monitoring

#### 4.1 Event Manager Configuration

**File:** `oprc-odgm/src/events/config.rs`

```rust
use crate::events::types::SerializationFormat;

#[derive(Debug, Clone)]
pub struct EventConfig {
    pub max_trigger_depth: u32,      // Prevent infinite loops
    pub trigger_timeout_ms: u64,     // Timeout for trigger execution
    pub enable_event_logging: bool,  // Log all event executions
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
```

#### 4.2 Integration with ODGM Configuration

**File:** `oprc-odgm/src/lib.rs` (modifications)

```rust
#[derive(Envconfig, Clone, Debug)]
pub struct OdgmConfig {
    // ...existing fields...
    
    #[envconfig(from = "ODGM_EVENTS_ENABLED", default = "true")]
    pub events_enabled: bool,
    
    #[envconfig(from = "ODGM_MAX_TRIGGER_DEPTH", default = "10")]
    pub max_trigger_depth: u32,
    
    #[envconfig(from = "ODGM_TRIGGER_TIMEOUT_MS", default = "30000")]
    pub trigger_timeout_ms: u64,
}
```

### Phase 5: Testing and Validation

#### 5.1 Unit Tests

**File:** `oprc-odgm/src/events/tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_function_trigger_execution() {
        // Test function completion triggers
    }

    #[tokio::test]
    async fn test_data_change_triggers() {
        // Test data create/update/delete triggers
    }

    #[tokio::test]
    async fn test_trigger_loop_prevention() {
        // Test prevention of infinite trigger loops
    }

    #[tokio::test]
    async fn test_event_persistence() {
        // Test that events are stored and retrieved correctly
    }
}
```

#### 5.2 Integration Tests

**File:** `oprc-odgm/tests/event_integration_tests.rs`

```rust
#[tokio::test]
async fn test_end_to_end_event_flow() {
    // Create ODGM instance with events enabled
    // Set up object with triggers
    // Invoke function and verify triggers execute
    // Modify data and verify data triggers execute
}
```

## Shard Options for Events

Add these new shard options to the README:

### Event Options

* `events_enabled` - Enable/disable event processing for this shard. Default is `true`.
* `max_trigger_depth` - Maximum depth of nested trigger executions to prevent loops. Default is `10`.
* `trigger_timeout_ms` - Timeout for individual trigger execution in milliseconds. Default is `30000`.
* `max_concurrent_triggers` - Maximum number of triggers that can execute concurrently. Default is `100`.
* `event_logging_enabled` - Enable detailed logging of event executions. Default is `true`.

## API Endpoints to Implement

Based on reference.adoc, these endpoints should be implemented:

1. **Object change subscription**: `SUB oprc/<class>/<partition>/objects/<object_id>/change`
2. **Enhanced object operations** that trigger events automatically

## Migration Strategy

1. **Phase 1**: Implement core event infrastructure without breaking existing functionality
2. **Phase 2**: Add event integration points with feature flags
3. **Phase 3**: Add persistence and configuration
4. **Phase 4**: Enable by default with comprehensive testing
5. **Phase 5**: Add monitoring and optimization

## Performance Considerations

1. **Pure Async Execution**: All trigger executions use Zenoh's fire-and-forget PUT operations with zero result handling
2. **Batching**: Group multiple triggers for the same event
3. **Rate Limiting**: Prevent trigger storms with configurable limits
4. **Memory Management**: Clean up completed trigger contexts immediately
5. **Circuit Breaker**: Disable triggers temporarily if they fail repeatedly

### Zenoh Async Invocation Benefits

The pure async approach using Zenoh's invocation API provides maximum performance:

- **Zero Blocking**: Event processing never waits for trigger completion
- **Zero Overhead**: No result subscriptions, no monitoring, no tracking
- **Maximum Throughput**: Events processed at the speed of memory access
- **Perfect Isolation**: Trigger execution completely decoupled from event processing
- **Infinite Scalability**: Zenoh handles unlimited concurrent trigger dispatching
- **Resource Efficiency**: Minimal memory footprint, no connection pooling, no result handling

The pure fire-and-forget pattern ensures:
1. Events are processed at maximum possible speed
2. Triggers execute independently without any back-pressure
3. System remains perfectly responsive regardless of trigger load
4. Failed triggers have zero impact on system performance
5. No resource leaks from result handling or monitoring

## Security Considerations

1. **Authorization**: Verify trigger targets have permission to execute
2. **Resource Limits**: Prevent triggers from consuming excessive resources
3. **Input Validation**: Sanitize trigger payloads
4. **Audit Logging**: Log all trigger executions for security review

## Summary

This implementation plan provides a comprehensive foundation for the event triggering system while maintaining compatibility with the existing OPRC-ODGM architecture. The design emphasizes:

- **Performance**: Pure async execution via Zenoh with zero blocking overhead
- **Type Safety**: Protobuf-generated types with dual JSON/protobuf serialization
- **Simplicity**: Clean separation of concerns with minimal configuration
- **Integration**: Seamless integration with existing shard and invocation systems
- **Extensibility**: Structured design that supports future enhancements

The system is ready for implementation following the phased approach outlined above.
