// Criterion benchmark for OPRC-ODGM event system
// Run with: cargo bench --bench event_bench

use criterion::{Criterion, criterion_group, criterion_main};
use std::{collections::HashMap, hint::black_box};

use oprc_odgm::events::types::{
    EventContext, EventType, SerializationFormat, TriggerExecutionContext,
    create_trigger_payload, serialize_trigger_payload,
};
use oprc_pb::{DataTrigger, FuncTrigger, ObjectEvent, TriggerTarget};

// Helper function to create test data for benchmarks
fn create_test_object_event() -> ObjectEvent {
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

    object_event
        .func_trigger
        .insert("test_function".to_string(), func_trigger);

    object_event
}

fn create_test_event_context() -> EventContext {
    EventContext {
        object_id: 12345,
        class_id: "test_class".to_string(),
        partition_id: 1,
        event_type: EventType::DataUpdate(42),
        payload: Some(b"test payload".to_vec()),
        error_message: None,
    }
}

// Helper function extracted from EventManager for benchmarking
fn collect_matching_triggers_for_benchmark(
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

fn trigger_collection_benchmark(c: &mut Criterion) {
    let object_event = create_test_object_event();
    let event_type = EventType::DataCreate(100);

    c.bench_function("trigger_collection", |b| {
        b.iter(|| {
            let _triggers = collect_matching_triggers_for_benchmark(
                black_box(&object_event),
                black_box(&event_type),
            );
        })
    });
}

fn payload_serialization_json_benchmark(c: &mut Criterion) {
    let context = create_test_event_context();
    let target = TriggerTarget {
        cls_id: "perf_service".to_string(),
        partition_id: 1,
        object_id: None,
        fn_id: "perf_handler".to_string(),
        req_options: HashMap::new(),
    };

    let exec_context = TriggerExecutionContext {
        source_event: context,
        target,
    };

    let payload = create_trigger_payload(&exec_context);

    c.bench_function("payload_serialization_json", |b| {
        b.iter(|| {
            let _serialized = serialize_trigger_payload(
                black_box(&payload),
                black_box(SerializationFormat::Json),
            );
        })
    });
}

fn payload_serialization_protobuf_benchmark(c: &mut Criterion) {
    let context = create_test_event_context();
    let target = TriggerTarget {
        cls_id: "perf_service".to_string(),
        partition_id: 1,
        object_id: None,
        fn_id: "perf_handler".to_string(),
        req_options: HashMap::new(),
    };

    let exec_context = TriggerExecutionContext {
        source_event: context,
        target,
    };

    let payload = create_trigger_payload(&exec_context);

    c.bench_function("payload_serialization_protobuf", |b| {
        b.iter(|| {
            let _serialized = serialize_trigger_payload(
                black_box(&payload),
                black_box(SerializationFormat::Protobuf),
            );
        })
    });
}

fn event_context_creation_benchmark(c: &mut Criterion) {
    c.bench_function("event_context_creation", |b| {
        b.iter(|| {
            let _context = EventContext {
                object_id: black_box(12345),
                class_id: black_box("perf_test".to_string()),
                partition_id: black_box(1),
                event_type: black_box(EventType::DataUpdate(42)),
                payload: black_box(Some(vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])),
                error_message: black_box(None),
            };
        })
    });
}

criterion_group!(
    benches,
    trigger_collection_benchmark,
    payload_serialization_json_benchmark,
    payload_serialization_protobuf_benchmark,
    event_context_creation_benchmark
);
criterion_main!(benches);
