use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry, Layer};
use tracing_subscriber::fmt::format::FmtSpan;

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub service_name: String,
    pub log_level: String,
    pub json_format: bool,
    #[cfg(feature = "jaeger")]
    pub jaeger_endpoint: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            service_name: "oaas-service".to_string(),
            log_level: "info".to_string(),
            json_format: true,
            #[cfg(feature = "jaeger")]
            jaeger_endpoint: None,
        }
    }
}

pub fn setup_tracing(config: TracingConfig) -> Result<(), Box<dyn std::error::Error>> {
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
    
    let registry = Registry::default()
        .with(env_filter)
        .with(fmt_layer);
    
    // Add Jaeger tracing if endpoint is provided and feature is enabled
    #[cfg(feature = "jaeger")]
    if let Some(jaeger_endpoint) = config.jaeger_endpoint {
        let tracer = opentelemetry_jaeger::new_agent_pipeline()
            .with_service_name(&config.service_name)
            .with_endpoint(&jaeger_endpoint)
            .install_simple()?;
        
        let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
        registry = registry.with(telemetry);
    }
    
    registry.init();
    
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum TracingError {
    #[error("Tracing setup error: {0}")]
    Setup(String),
    
    #[cfg(feature = "jaeger")]
    #[error("Jaeger error: {0}")]
    Jaeger(#[from] opentelemetry::trace::TraceError),
}
