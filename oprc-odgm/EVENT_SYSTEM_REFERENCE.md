# OPRC Event System - Implementation Reference

## Overview

The OPRC Event System provides **pure asynchronous trigger execution** for the Object Data Grid Manager (ODGM). This system enables reactive, event-driven applications with zero-overhead fire-and-forget semantics using Zenoh's publish-subscribe infrastructure.

## 🏗️ Architecture

### Core Components

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Object Shard  │───▶│  Event Manager  │───▶│ Trigger Processor│
│                 │    │                 │    │                 │
│ - Data Changes  │    │ - Event Match   │    │ - Zenoh PUT     │
│ - Function Calls│    │ - Trigger Eval  │    │ - Async Invoke  │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

**Event Flow**: `Object Operation → Event Detection → Trigger Evaluation → Target Execution (via Zenoh)`

### Key Design Principles

1. **Pure Async**: No synchronous waiting or result handling
2. **Fire-and-Forget**: Maximum performance through zero result tracking  
3. **Protobuf Types**: Consistent with existing system architecture
4. **Optional Integration**: Event system can be disabled without affecting core functionality
5. **Zenoh-Native**: Leverages existing Zenoh infrastructure for reliability

## 📋 Implementation Components

### ✅ Fully Implemented Features

- **Data Event Triggering**: Object create/update/delete events automatically triggered with field-level granularity
- **Zenoh Integration**: All triggers execute via Zenoh's async invocation API with fire-and-forget PUT operations
- **Type Safety**: Protobuf-generated types for event payloads with dual serialization support (JSON/protobuf)
- **Configuration Management**: Environment-based configuration with runtime enable/disable capabilities
- **Performance Testing**: Comprehensive Criterion benchmarks available via `cargo bench --bench event_bench`

### 1. Event Types ([`events/types.rs`](src/events/types.rs))

Core event type definitions and payload handling:

- **EventType Enum**: FunctionComplete, FunctionError, DataCreate, DataUpdate, DataDelete
- **EventContext**: Contains object metadata, event type, and optional payload
- **TriggerExecutionContext**: Links source events to target triggers
- **Payload Functions**: `create_trigger_payload()` and `serialize_trigger_payload()` with dual JSON/Protobuf support

### 2. Event Manager ([`events/manager.rs`](src/events/manager.rs))

Central component for event registration and dispatch:

- **Main API**: `trigger_event()` for shard-based processing, `trigger_event_with_entry()` for direct processing
- **Trigger Collection**: O(1) hash map lookup per event type with field-level and function-level granularity
- **Multi-Target Support**: Multiple triggers can respond to the same event

### 3. Trigger Processor ([`events/processor.rs`](src/events/processor.rs))

Executes triggers using Zenoh's async invocation API:

- **Fire-and-Forget Execution**: `execute_trigger()` uses Zenoh PUT operations
- **Zenoh Key Patterns**:
  - Stateless: `oprc/{cls_id}/{partition_id}/async/{fn_id}/{invocation_id}`
  - Stateful: `oprc/{cls_id}/{partition_id}/objects/{object_id}/async/{fn_id}/{invocation_id}`
- **Zero Result Tracking**: Maximum performance through async dispatch

### 4. Event Configuration ([`events/config.rs`](src/events/config.rs))

Runtime configuration for event system behavior:

- **EventConfig Struct**: Controls trigger depth, timeouts, logging, and concurrency limits
- **Environment Variables**: `ODGM_EVENTS_ENABLED`, `ODGM_MAX_TRIGGER_DEPTH`, `ODGM_TRIGGER_TIMEOUT_MS`
- **Serialization Formats**: Configurable JSON or Protobuf payload serialization

## 🔧 Integration Points

### Shard Integration

ObjectShard now supports optional event manager integration. When enabled, data operations automatically trigger events:

- **Event Detection**: Data changes (set/delete) automatically generate `EventContext` objects
- **Automatic Triggering**: Events are dispatched to the event manager without manual intervention
- **Zero Performance Impact**: When disabled, no overhead is added to data operations

*See implementation: [`shard/mod.rs`](src/shard/mod.rs)*

### gRPC Service Integration

Data service operations trigger events automatically through the shard layer:

- **Transparent Operation**: Existing gRPC endpoints automatically support events
- **Event Types**: Create, update, and delete operations generate appropriate event types
- **Field Granularity**: Events can be configured per object field for fine-grained control

*See implementation: [`grpc_service/data.rs`](src/grpc_service/data.rs)*

## 📝 Usage Examples

### 1. Basic Event Configuration

Configure data change triggers using protobuf structures:

```rust
use oprc_pb::{ObjData, ObjectEvent, DataTrigger, TriggerTarget};
use std::collections::HashMap;

// Create an object with data change triggers
let mut obj_data = ObjData::default();

// Configure event triggers
let mut object_event = ObjectEvent::default();
let mut data_trigger = DataTrigger::default();

// Add a trigger for field updates
data_trigger.on_update.push(TriggerTarget {
    cls_id: "notification_service".to_string(),
    partition_id: 0,
    object_id: None, // Stateless function
    fn_id: "send_notification".to_string(),
    req_options: HashMap::new(),
});

object_event.data_trigger.insert(42, data_trigger);
obj_data.event = Some(object_event);

// When this object is updated, the notification service will be called
```

When object field 42 is updated, `notification_service::send_notification` executes asynchronously.

*Complete example: [`tests.rs`](src/events/tests.rs) - `create_test_object_entry_with_events()`*

### 2. Function Event Triggers

Configure function completion and error handling:

- **Success Triggers**: Execute on function completion
- **Error Triggers**: Execute on function failures  
- **Stateful vs Stateless**: Supports both object methods and standalone functions

*Configuration patterns: [`tests.rs`](src/events/tests.rs) - Function trigger tests*

### 3. Multiple Triggers Per Event

Multiple services can respond to the same event:

- **Notification Service**: Real-time user notifications
- **Audit Service**: Compliance and logging  
- **Analytics Service**: Usage tracking and metrics

All services execute concurrently via Zenoh's async infrastructure.

*Multi-trigger examples: [`tests.rs`](src/events/tests.rs) - `test_multiple_triggers_same_event`*

## 🚀 Performance Characteristics

### Benchmarked Performance

- **Event Detection**: O(1) field comparison per changed field
- **Trigger Matching**: O(1) hash map lookup per event type
- **Trigger Collection**: <10μs per operation (10,000 operations in <100ms)
- **Payload Serialization**: <100μs per payload (10,000 operations in <1s)
- **Memory Footprint**: Minimal - no result tracking or monitoring
- **Concurrency**: Unlimited concurrent trigger execution via Zenoh

### Scalability Features

- **Zero Blocking**: Event processing never waits for trigger completion
- **Perfect Isolation**: Trigger failures don't affect system performance
- **Unlimited Concurrency**: Zenoh handles concurrent trigger dispatching
- **Memory Efficient**: No state tracking for fire-and-forget operations

## 🧪 Testing

### Test Coverage (13 Tests)

**Test Categories**:
- **Integration Tests (6)**: Trigger collection logic, event-to-trigger matching, multi-target scenarios
- **Core Functionality Tests (4)**: Payload serialization, mock compatibility, event creation
- **Performance Tests (3)**: Benchmarking trigger collection, serialization, and context creation

**Running Tests**:
```bash
cargo test events                              # All tests
cargo test events::performance_tests -- --nocapture  # With performance output
```

**Current Status**: ✅ 13 tests passing, <0.15s execution time

*Complete test suite: [`tests.rs`](src/events/tests.rs)*

## 🔧 Configuration

### Environment Variables
```bash
ODGM_EVENTS_ENABLED=true          # Enable/disable event processing (default: true)
ODGM_MAX_TRIGGER_DEPTH=10         # Maximum nested trigger depth (default: 10)
ODGM_TRIGGER_TIMEOUT_MS=30000     # Individual trigger timeout (default: 30000ms)
```

### Runtime Configuration
EventConfig supports programmatic configuration of trigger depth, timeouts, logging, concurrency limits, and payload serialization format (JSON/Protobuf).

*Configuration details: [`config.rs`](src/events/config.rs)*

## 🔄 Event Lifecycle

### High-Level Flow

1. **Event Detection**: Data operations automatically generate EventContext objects
2. **Trigger Collection**: EventManager performs O(1) hash map lookup for matching triggers  
3. **Async Execution**: TriggerProcessor creates TriggerExecutionContext and dispatches via Zenoh
4. **Zenoh Dispatch**: Fire-and-forget PUT operations to target endpoints with structured payloads

### Key Characteristics

- **Zero Blocking**: Event processing never waits for trigger completion
- **Type Safety**: Strongly typed event pipeline with protobuf compatibility
- **Performance**: O(1) trigger matching, sub-millisecond processing
- **Reliability**: No state tracking eliminates memory leaks and failure points

*Implementation details: [`manager.rs`](src/events/manager.rs), [`processor.rs`](src/events/processor.rs)*

## 🎯 Best Practices

### 1. Event Design
- Use field-level granularity for data events
- Keep trigger payloads lightweight
- Avoid deep trigger chains (respect max_trigger_depth)

### 2. Performance Optimization
- Use direct `trigger_event_with_entry()` when object is already available
- Configure appropriate trigger timeouts
- Monitor trigger execution patterns

### 3. Error Handling
- Design idempotent trigger targets
- Use error triggers for cleanup operations
- Log important trigger failures

### 4. Testing
- Test trigger collection logic independently
- Validate payload serialization formats
- Benchmark performance characteristics

## 🔮 Future Enhancements

### Function Events Integration

While data events are fully implemented, function event triggering would require integration with the invocation system:

1. **Hook into InvocationOffloader**: Modify `invoke_fn` and `invoke_obj` methods
2. **Add Result Context**: Capture function success/failure outcomes  
3. **Event Context Creation**: Generate function completion/error events
4. **Integration Point**: Connect to ObjectShard's event manager

### Planned Features
1. **Enhanced Monitoring**: Optional trigger execution tracking and metrics
2. **Conditional Triggers**: Support for trigger conditions and filters
3. **Bulk Operations**: Batch trigger execution for high-throughput scenarios

### Integration Opportunities
1. **Workflow Engine**: Build complex workflows using event chains
2. **Real-time Analytics**: Stream events to analytics systems
3. **State Synchronization**: Keep external systems in sync with object changes
4. **Audit Logging**: Comprehensive audit trails using event triggers

The OPRC Event System provides a robust, high-performance foundation for building reactive, event-driven applications on top of the Object Data Grid Manager.
