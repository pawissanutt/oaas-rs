use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{
    EnvFilter, Layer, Registry, filter, layer::SubscriberExt,
    util::SubscriberInitExt,
};

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub service_name: String,
    pub log_level: String,
    pub json_format: bool,
    #[cfg(feature = "otlp")]
    pub otlp_endpoint: Option<String>,
    /// Enable OTLP trace export (default: true when endpoint is set)
    #[cfg(feature = "otlp")]
    pub enable_otlp_traces: bool,
    /// Enable OTLP log export (default: true when endpoint is set)
    #[cfg(feature = "otlp")]
    pub enable_otlp_logs: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "oaas-service".to_string(),
            log_level: "info".to_string(),
            json_format: true,
            #[cfg(feature = "otlp")]
            otlp_endpoint: None,
            #[cfg(feature = "otlp")]
            enable_otlp_traces: true,
            #[cfg(feature = "otlp")]
            enable_otlp_logs: true,
        }
    }
}

/// Create TracingConfig from environment variables.
/// Env vars:
/// - OTEL_EXPORTER_OTLP_ENDPOINT: OTLP endpoint URL
/// - OTEL_TRACES_ENABLED: "true"/"false" to enable/disable trace export (default: true)
/// - OTEL_LOGS_ENABLED: "true"/"false" to enable/disable log export (default: true)
#[cfg(feature = "otlp")]
impl TracingConfig {
    pub fn from_env(
        service_name: &str,
        log_level: &str,
        json_format: bool,
    ) -> Self {
        let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .ok()
            .filter(|v| !v.trim().is_empty());

        let enable_traces = std::env::var("OTEL_TRACES_ENABLED")
            .map(|v| v.to_lowercase() != "false" && v != "0")
            .unwrap_or(true);

        let enable_logs = std::env::var("OTEL_LOGS_ENABLED")
            .map(|v| v.to_lowercase() != "false" && v != "0")
            .unwrap_or(true);

        Self {
            service_name: service_name.to_string(),
            log_level: log_level.to_string(),
            json_format,
            otlp_endpoint,
            enable_otlp_traces: enable_traces,
            enable_otlp_logs: enable_logs,
        }
    }
}

pub fn setup_tracing(
    config: TracingConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Target used by tonic-tracing-opentelemetry for span propagation
    const OTEL_TRACING_TARGET: &str = "otel::tracing";

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_file(true)
        .with_line_number(true);

    // Filter out otel::tracing spans from stdout/stderr - they're only for OTLP export
    let fmt_filter = filter::FilterFn::new(|metadata| {
        metadata.target() != OTEL_TRACING_TARGET
    });

    let fmt_layer = if config.json_format {
        fmt_layer.json().with_filter(fmt_filter).boxed()
    } else {
        fmt_layer.with_filter(fmt_filter).boxed()
    };

    // Build env filter with otel::tracing=trace appended to ensure trace propagation works
    // This is required for tonic-tracing-opentelemetry to create spans for distributed tracing
    let log_level_with_otel =
        format!("{},{}=trace", config.log_level, OTEL_TRACING_TARGET);
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&log_level_with_otel));

    let registry = Registry::default().with(env_filter).with(fmt_layer);

    // Add OTLP tracing and logs if endpoint is provided and feature is enabled
    #[cfg(feature = "otlp")]
    if let Some(ref otlp_endpoint) = config.otlp_endpoint {
        // Initialize OTLP trace exporter if enabled
        let tracer_result = if config.enable_otlp_traces {
            crate::otel_exporter::init_otlp_tracing(
                &config.service_name,
                Some(otlp_endpoint),
            )
            .map(Some)
        } else {
            Ok(None)
        };

        // Initialize OTLP logs exporter if enabled
        let logs_result = if config.enable_otlp_logs {
            crate::otel_exporter::init_otlp_logs(
                &config.service_name,
                Some(otlp_endpoint),
            )
            .map(Some)
        } else {
            Ok(None)
        };

        match (tracer_result, logs_result) {
            (Ok(Some(tracer)), Ok(Some(logger_provider))) => {
                println!(
                    "Successfully initialized OTLP tracing and logs to {}",
                    otlp_endpoint
                );
                let telemetry =
                    tracing_opentelemetry::layer().with_tracer(tracer);
                let otel_log_layer = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);

                registry.with(telemetry).with(otel_log_layer).init();
                return Ok(());
            }
            (Ok(Some(tracer)), Ok(None)) => {
                println!(
                    "Successfully initialized OTLP tracing to {} (logs disabled)",
                    otlp_endpoint
                );
                let telemetry =
                    tracing_opentelemetry::layer().with_tracer(tracer);
                registry.with(telemetry).init();
                return Ok(());
            }
            (Ok(None), Ok(Some(logger_provider))) => {
                println!(
                    "Successfully initialized OTLP logs to {} (traces disabled)",
                    otlp_endpoint
                );
                let otel_log_layer = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);
                registry.with(otel_log_layer).init();
                return Ok(());
            }
            (Ok(None), Ok(None)) => {
                println!(
                    "OTLP endpoint configured but both traces and logs disabled"
                );
                // Fallthrough to standard registry init
            }
            (Ok(Some(tracer)), Err(e)) => {
                eprintln!(
                    "Failed to initialize OTLP logs: {}. Continuing with traces only.",
                    e
                );
                let telemetry =
                    tracing_opentelemetry::layer().with_tracer(tracer);
                registry.with(telemetry).init();
                return Ok(());
            }
            (Err(e), Ok(Some(logger_provider))) => {
                eprintln!(
                    "Failed to initialize OTLP traces: {}. Continuing with logs only.",
                    e
                );
                let otel_log_layer = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&logger_provider);
                registry.with(otel_log_layer).init();
                return Ok(());
            }
            (Ok(None), Err(e)) => {
                eprintln!(
                    "Failed to initialize OTLP logs (traces disabled): {}. Falling back to standard logging.",
                    e
                );
                // Fallthrough to standard registry init below
            }
            (Err(e), _) => {
                eprintln!(
                    "Failed to initialize OTLP tracing: {}. Falling back to standard logging.",
                    e
                );
                // Fallthrough to standard registry init below
            }
        }
    }

    registry.init();

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum TracingError {
    #[error("Tracing setup error: {0}")]
    Setup(String),

    #[cfg(feature = "otlp")]
    #[error("OTLP error: {0}")]
    Otlp(#[from] opentelemetry_sdk::trace::TraceError),
}
