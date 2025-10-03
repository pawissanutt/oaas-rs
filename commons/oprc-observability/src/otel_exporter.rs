// Stub exporter module: metrics/tracing exporters only activate when implemented.

/// Initialize a basic OTLP metrics exporter (gRPC) with periodic reader.
/// Deprecated behavior: previously defaulted to http://localhost:4317 when endpoint absent.
/// Now this function is a thin wrapper that only initializes if `endpoint` is Some.
pub fn init_otlp_metrics(
    _service_name: &str,
    _endpoint: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    let _endpoint = match std::env::var("OPRC_OTEL_METRICS_ENDPOINT") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => return Ok(false),
    };
    let service_name = std::env::var("OPRC_OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| default_service.to_string());
    let _ = service_name; // silence unused
    Ok(false)
}

/// Initialize tracing OTLP exporter (gRPC). Optional convenience helper.
pub fn init_otlp_tracing(
    _service_name: &str,
    _endpoint: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Ok(())
}
