use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource,
    logs::SdkLoggerProvider,
    metrics::{PeriodicReader, SdkMeterProvider},
    propagation::TraceContextPropagator,
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
///  - OTEL_EXPORTER_OTLP_ENDPOINT or OTEL_EXPORTER_OTLP_METRICS_ENDPOINT (required to enable)
///  - OTEL_SERVICE_NAME (optional override)
///  - OTEL_METRICS_PERIOD_SECS (optional, default 30)
pub fn init_otlp_metrics_if_configured(
    default_service: &str,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    // Check for metrics-specific endpoint first, then fallback to general OTLP endpoint
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_METRICS_ENDPOINT")
        .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
        .ok()
        .filter(|v| !v.trim().is_empty());
    let endpoint = match endpoint {
        Some(v) => v,
        None => return Ok(false),
    };
    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| default_service.to_string());

    init_otlp_metrics(&service_name, Some(&endpoint))?;
    Ok(true)
}

/// Initialize tracing OTLP exporter (gRPC).
/// Sampler can be configured via OTEL_TRACES_SAMPLER env var:
/// - "always_on" (default): Sample all traces
/// - "always_off": Sample no traces
/// - "traceidratio": Sample based on OTEL_TRACES_SAMPLER_ARG (0.0-1.0, default 0.1)
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

    // Configure sampler from OTEL_TRACES_SAMPLER env var
    // We wrap the sampler in ParentBased so that:
    // 1. If there's a parent span from incoming trace context, respect its sampling decision
    // 2. If there's no parent (root span), use the configured sampler
    let base_sampler: Box<dyn opentelemetry_sdk::trace::ShouldSample> =
        match std::env::var("OTEL_TRACES_SAMPLER")
            .unwrap_or_else(|_| "always_on".to_string())
            .to_lowercase()
            .as_str()
        {
            "always_off" => Box::new(Sampler::AlwaysOff),
            "traceidratio" => {
                let ratio = std::env::var("OTEL_TRACES_SAMPLER_ARG")
                    .ok()
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(0.1);
                Box::new(Sampler::TraceIdRatioBased(ratio))
            }
            // Handle "parentbased_always_on" which is the OTEL SDK standard
            "parentbased_always_on" => {
                // This is already parent-based, just use AlwaysOn as root sampler
                Box::new(Sampler::AlwaysOn)
            }
            _ => Box::new(Sampler::AlwaysOn), // "always_on" or any other value
        };

    // Wrap in ParentBased to respect incoming trace context sampling decisions
    let sampler = Sampler::ParentBased(base_sampler);

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer(service_name.to_string());

    // Set global tracer provider
    opentelemetry::global::set_tracer_provider(provider);

    // Set global text map propagator for distributed tracing context propagation
    opentelemetry::global::set_text_map_propagator(
        TraceContextPropagator::new(),
    );

    Ok(tracer)
}

/// Initialize tracing OTLP exporter if configured in env.
/// Env vars:
/// - OTEL_EXPORTER_OTLP_ENDPOINT or OTEL_EXPORTER_OTLP_TRACES_ENDPOINT (required)
/// - OTEL_SERVICE_NAME (optional)
pub fn init_otlp_tracing_if_configured(
    default_service: &str,
) -> Result<
    Option<opentelemetry_sdk::trace::Tracer>,
    Box<dyn std::error::Error + Send + Sync>,
> {
    // Check for traces-specific endpoint first, then fallback to general OTLP endpoint
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT")
        .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
        .ok()
        .filter(|v| !v.trim().is_empty());
    let endpoint = match endpoint {
        Some(v) => v,
        None => return Ok(None),
    };
    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| default_service.to_string());

    let tracer = init_otlp_tracing(&service_name, Some(&endpoint))?;
    Ok(Some(tracer))
}

/// Initialize OTLP logs exporter (gRPC) and return a LoggerProvider.
/// This should be used with opentelemetry-appender-tracing to bridge tracing logs to OTLP.
pub fn init_otlp_logs(
    service_name: &str,
    endpoint: Option<&str>,
) -> Result<SdkLoggerProvider, Box<dyn std::error::Error + Send + Sync>> {
    let endpoint = endpoint.unwrap_or("http://localhost:4317");

    let resource = Resource::builder()
        .with_attribute(KeyValue::new("service.name", service_name.to_string()))
        .build();

    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;

    let provider = SdkLoggerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    Ok(provider)
}

/// Initialize OTLP logs exporter if configured in env.
/// Returns the LoggerProvider that should be used with OpenTelemetryTracingBridge.
/// Env vars:
/// - OTEL_EXPORTER_OTLP_ENDPOINT or OTEL_EXPORTER_OTLP_LOGS_ENDPOINT (required)
/// - OTEL_SERVICE_NAME (optional)
pub fn init_otlp_logs_if_configured(
    default_service: &str,
) -> Result<Option<SdkLoggerProvider>, Box<dyn std::error::Error + Send + Sync>>
{
    // Check for logs-specific endpoint first, then fallback to general OTLP endpoint
    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_LOGS_ENDPOINT")
        .or_else(|_| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT"))
        .ok()
        .filter(|v| !v.trim().is_empty());
    let endpoint = match endpoint {
        Some(v) => v,
        None => return Ok(None),
    };
    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| default_service.to_string());

    let provider = init_otlp_logs(&service_name, Some(&endpoint))?;
    Ok(Some(provider))
}
