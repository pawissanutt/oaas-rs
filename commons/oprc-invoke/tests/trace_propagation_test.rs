//! Integration tests for trace context propagation between Gateway and ODGM.
//!
//! These tests verify that trace context injected by the Gateway can be
//! correctly extracted by the ODGM handler, ensuring distributed tracing works.

use opentelemetry::global;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::{TraceContextExt, TracerProvider};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::collections::HashMap;
use tracing::{Span, info_span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{Registry, layer::SubscriberExt};

/// Helper to inject trace context (simulates Gateway side)
fn inject_trace_context(options: &mut HashMap<String, String>) {
    struct MapInjector<'a>(&'a mut HashMap<String, String>);
    impl Injector for MapInjector<'_> {
        fn set(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }
    }

    let ctx = Span::current().context();
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&ctx, &mut MapInjector(options));
    });
}

/// Helper to extract trace context (simulates ODGM side)
fn extract_trace_context(
    options: &HashMap<String, String>,
) -> opentelemetry::Context {
    struct MapExtractor<'a>(&'a HashMap<String, String>);
    impl Extractor for MapExtractor<'_> {
        fn get(&self, key: &str) -> Option<&str> {
            self.0.get(key).map(|s| s.as_str())
        }
        fn keys(&self) -> Vec<&str> {
            self.0.keys().map(|k| k.as_str()).collect()
        }
    }

    global::get_text_map_propagator(|propagator| {
        propagator.extract(&MapExtractor(options))
    })
}

/// Setup test environment with tracer and propagator
fn setup_test_otel() -> SdkTracerProvider {
    // Set up the W3C TraceContext propagator
    global::set_text_map_propagator(TraceContextPropagator::new());

    // Create a simple in-memory tracer provider for testing
    SdkTracerProvider::builder().build()
}

#[test]
fn test_trace_context_propagation_with_real_span() {
    // Setup OpenTelemetry
    let provider = setup_test_otel();
    let tracer = provider.tracer("test-tracer");

    // Setup tracing-opentelemetry layer
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = Registry::default().with(telemetry_layer);

    tracing::subscriber::with_default(subscriber, || {
        // Create a parent span (simulates Gateway's invoke_fn span)
        let gateway_span =
            info_span!("gateway_invoke_fn", cls = "test-class", func = "echo");
        let _guard = gateway_span.enter();

        // Get the trace ID from the current span
        let gateway_ctx = Span::current().context();
        let gateway_span_ctx = gateway_ctx.span().span_context().clone();

        println!(
            "Gateway span - trace_id: {:?}, span_id: {:?}",
            gateway_span_ctx.trace_id(),
            gateway_span_ctx.span_id()
        );

        // Inject trace context into options (what Gateway does)
        let mut options: HashMap<String, String> = HashMap::new();
        inject_trace_context(&mut options);

        // Verify traceparent was injected
        assert!(
            options.contains_key("traceparent"),
            "traceparent should be injected into options"
        );
        println!("Injected traceparent: {:?}", options.get("traceparent"));

        // Extract trace context (what ODGM does)
        let extracted_ctx = extract_trace_context(&options);
        let extracted_span_ctx = extracted_ctx.span().span_context().clone();

        println!(
            "Extracted span - trace_id: {:?}, span_id: {:?}",
            extracted_span_ctx.trace_id(),
            extracted_span_ctx.span_id()
        );

        // Verify the trace IDs match (this confirms trace context was propagated correctly)
        assert!(
            extracted_span_ctx.is_valid(),
            "Extracted span context should be valid"
        );
        assert_eq!(
            gateway_span_ctx.trace_id(),
            extracted_span_ctx.trace_id(),
            "Trace IDs should match between Gateway and ODGM"
        );

        // Note: span_id will be different because the extracted context is the parent,
        // not a new child span. The gateway span_id becomes the parent span_id for ODGM.
        assert_eq!(
            gateway_span_ctx.span_id(),
            extracted_span_ctx.span_id(),
            "Parent span ID should be the gateway span ID"
        );
    });
}

#[test]
fn test_child_span_inherits_trace_id() {
    // Setup OpenTelemetry
    let provider = setup_test_otel();
    let tracer = provider.tracer("test-tracer");

    // Setup tracing-opentelemetry layer
    let telemetry_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let subscriber = Registry::default().with(telemetry_layer);

    tracing::subscriber::with_default(subscriber, || {
        // Create a parent span (simulates Gateway's invoke_fn span)
        let gateway_span = info_span!("gateway_invoke_fn");
        let _guard = gateway_span.enter();

        let gateway_ctx = Span::current().context();
        let gateway_trace_id = gateway_ctx.span().span_context().trace_id();

        // Inject trace context into options
        let mut options: HashMap<String, String> = HashMap::new();
        inject_trace_context(&mut options);

        // Simulate ODGM side: extract and set parent
        let extracted_ctx = extract_trace_context(&options);

        // Create ODGM span and set extracted context as parent
        let odgm_span = info_span!("odgm_handle_invoke");
        let _ = odgm_span.set_parent(extracted_ctx);
        let _odgm_guard = odgm_span.enter();

        // Verify the ODGM span has the same trace ID
        let odgm_ctx = Span::current().context();
        let odgm_trace_id = odgm_ctx.span().span_context().trace_id();

        println!("Gateway trace_id: {:?}", gateway_trace_id);
        println!("ODGM trace_id: {:?}", odgm_trace_id);

        assert_eq!(
            gateway_trace_id, odgm_trace_id,
            "ODGM span should have the same trace ID as Gateway span"
        );
    });
}

#[test]
fn test_empty_options_produces_invalid_context() {
    // Setup
    global::set_text_map_propagator(TraceContextPropagator::new());

    let options: HashMap<String, String> = HashMap::new();
    let ctx = extract_trace_context(&options);

    // Without any trace context, the extracted span should be invalid
    assert!(
        !ctx.span().span_context().is_valid(),
        "Empty options should produce invalid span context"
    );
}
