use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    metrics::{PeriodicReader, SdkMeterProvider},
    trace::{Sampler, SdkTracerProvider},
};
use std::time::Duration;

/// Initialize a basic OTLP metrics exporter (gRPC) with periodic reader.
pub fn init_otlp_metrics(
    _service_name: &str,
    endpoint: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = endpoint.unwrap_or("http://localhost:4317");

    let resource = Resource::builder()
        .with_attribute(KeyValue::new(
            "service.name",
            _service_name.to_string(),
        ))
        .build();

    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let reader = PeriodicReader::builder(exporter)
        .with_interval(Duration::from_secs(30))
        .build();

    let provider = SdkMeterProvider::builder()
        .with_resource(resource)
        .with_reader(reader)
        .build();

    opentelemetry::global::set_meter_provider(provider);
    Ok(())
}

/// Initialize OTLP metrics exporter only when environment configuration is present.
/// Returns Ok(true) if exporter installed, Ok(false) if skipped.
/// Env vars:
///  - OPRC_OTEL_METRICS_ENDPOINT (required to enable)
///  - OPRC_OTEL_SERVICE_NAME (optional override)
///  - OPRC_OTEL_METRICS_PERIOD_SECS (optional, default 30)
pub fn init_otlp_metrics_if_configured(
    default_service: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = match std::env::var("OPRC_OTEL_METRICS_ENDPOINT") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(false),
    };
    let service_name = std::env::var("OPRC_OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| default_service.to_string());

    init_otlp_metrics(&service_name, Some(&endpoint))?;
    Ok(true)
}

/// Initialize tracing OTLP exporter (gRPC).
pub fn init_otlp_tracing(
    service_name: &str,
    endpoint: Option<&str>,
) -> Result<
    opentelemetry_sdk::trace::Tracer,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let endpoint = endpoint.unwrap_or("http://localhost:4317");

    let resource = Resource::builder()
        .with_attribute(KeyValue::new("service.name", service_name.to_string()))
        .build();

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(Sampler::TraceIdRatioBased(0.1))
        .with_resource(resource)
        .build();

    let tracer = provider.tracer(service_name.to_string());

    // Set global tracer provider
    opentelemetry::global::set_tracer_provider(provider);

    Ok(tracer)
}

/// Initialize tracing OTLP exporter if configured in env.
/// Env vars:
/// - OPRC_OTEL_TRACING_ENDPOINT (required)
/// - OPRC_OTEL_SERVICE_NAME (optional)
pub fn init_otlp_tracing_if_configured(
    default_service: &str,
) -> Result<
    Option<opentelemetry_sdk::trace::Tracer>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    let endpoint = match std::env::var("OPRC_OTEL_TRACING_ENDPOINT") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(None),
    };
    let service_name = std::env::var("OPRC_OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| default_service.to_string());

    let tracer = init_otlp_tracing(&service_name, Some(&endpoint))?;
    Ok(Some(tracer))
}
