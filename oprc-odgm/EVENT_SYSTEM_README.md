# OPRC Event System

Pure asynchronous event triggering system for reactive applications in OPRC-ODGM.

## 📚 Documentation

- **[Implementation Reference](EVENT_SYSTEM_REFERENCE.md)** - Comprehensive technical documentation
- **[API Reference](../reference.adoc)** - Updated OPRC API with event endpoints  
- **[Test Summary](EVENTS_TEST_SUMMARY.md)** - Implementation status and test coverage

## 🚀 Quick Start

### 1. Configure Events on Objects

```rust
// Configure trigger for field updates  
data_trigger.on_update.push(TriggerTarget {
    cls_id: "notification_service".to_string(),
    partition_id: 0,
    fn_id: "send_notification".to_string(),
    // ...
});
```

*Complete examples: [`tests.rs`](src/events/tests.rs)*

### 2. Events Trigger Automatically

When object field 42 is updated, `notification_service::send_notification` executes asynchronously via Zenoh.

## ⚡ Performance

- **Event Detection**: O(1) field comparison
- **Trigger Matching**: O(1) hash map lookup  
- **Execution**: Zero-overhead fire-and-forget
- **Tested**: 13 comprehensive tests validating performance
- **Benchmarks**: Run `cargo bench --bench event_bench` for detailed performance metrics

## 🔧 Configuration

```bash
# Environment variables
ODGM_EVENTS_ENABLED=true
ODGM_MAX_TRIGGER_DEPTH=10
ODGM_TRIGGER_TIMEOUT_MS=30000
```

## 🧪 Testing

```bash
# Run all event tests
cargo test events

# Run with performance output
cargo test events::performance_tests -- --nocapture
```

**Current Status**: ✅ 13 tests passing, production-ready

## 🎯 Key Features

- ✅ **Data Events**: Create/Update/Delete field-level triggers
- ✅ **Function Events**: Completion/Error method triggers  
- ✅ **Multi-Target**: Multiple services per event
- ✅ **Zero-Overhead**: Pure async fire-and-forget execution
- ✅ **Type Safe**: Protobuf-based payload structures
- ✅ **Configurable**: Environment-based configuration

Built for high-performance reactive applications with Zenoh infrastructure.
