use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{
    EnvFilter,
    Layer,
    Registry,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    // Added for OpenTelemetry tracing
};

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub service_name: String,
    pub log_level: String,
    pub json_format: bool,
    #[cfg(feature = "otlp")]
    pub otlp_endpoint: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "oaas-service".to_string(),
            log_level: "info".to_string(),
            json_format: true,
            #[cfg(feature = "otlp")]
            otlp_endpoint: None,
        }
    }
}

pub fn setup_tracing(
    config: TracingConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_file(true)
        .with_line_number(true);

    let fmt_layer = if config.json_format {
        fmt_layer.json().boxed()
    } else {
        fmt_layer.boxed()
    };

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let registry = Registry::default().with(env_filter).with(fmt_layer);

    // Add OTLP tracing if endpoint is provided and feature is enabled
    // Add OTLP tracing if endpoint is provided and feature is enabled
    #[cfg(feature = "otlp")]
    if let Some(otlp_endpoint) = config.otlp_endpoint {
        match crate::otel_exporter::init_otlp_tracing(
            &config.service_name,
            Some(&otlp_endpoint),
        ) {
            Ok(tracer) => {
                println!(
                    "Successfully initialized OTLP tracing to {}",
                    otlp_endpoint
                );
                let telemetry =
                    tracing_opentelemetry::layer().with_tracer(tracer);
                registry.with(telemetry).init();
                return Ok(());
            }
            Err(e) => {
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
