use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};
use prometheus::{Counter, Gauge, Histogram, HistogramOpts, Opts, Registry};

#[derive(Clone)]
pub struct ServiceMetrics {
    pub requests_total: Counter,
    pub request_duration: Histogram,
    pub active_connections: Gauge,
    pub errors_total: Counter,
}

impl ServiceMetrics {
    pub fn new(
        service_name: &str,
        registry: &Registry,
    ) -> Result<Self, prometheus::Error> {
        let requests_total = Counter::with_opts(Opts::new(
            format!("{}_requests_total", service_name),
            format!("Total requests processed by {}", service_name),
        ))?;

        let request_duration = Histogram::with_opts(HistogramOpts::new(
            format!("{}_request_duration_seconds", service_name),
            format!("Request duration for {}", service_name),
        ))?;

        let active_connections = Gauge::with_opts(Opts::new(
            format!("{}_active_connections", service_name),
            format!("Active connections to {}", service_name),
        ))?;

        let errors_total = Counter::with_opts(Opts::new(
            format!("{}_errors_total", service_name),
            format!("Total errors in {}", service_name),
        ))?;

        registry.register(Box::new(requests_total.clone()))?;
        registry.register(Box::new(request_duration.clone()))?;
        registry.register(Box::new(active_connections.clone()))?;
        registry.register(Box::new(errors_total.clone()))?;

        Ok(ServiceMetrics {
            requests_total,
            request_duration,
            active_connections,
            errors_total,
        })
    }

    pub fn record_request(&self) {
        self.requests_total.inc();
    }

    pub fn record_request_duration(&self, duration: f64) {
        self.request_duration.observe(duration);
    }

    pub fn increment_active_connections(&self) {
        self.active_connections.inc();
    }

    pub fn decrement_active_connections(&self) {
        self.active_connections.dec();
    }

    pub fn record_error(&self) {
        self.errors_total.inc();
    }
}

// Metrics endpoint handler
async fn metrics_handler() -> impl IntoResponse {
    use prometheus::TextEncoder;

    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();

    match encoder.encode_to_string(&metric_families) {
        Ok(output) => (
            StatusCode::OK,
            [("content-type", "text/plain; charset=utf-8")],
            output,
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to encode metrics",
        )
            .into_response(),
    }
}

pub fn setup_metrics_server(
    port: u16,
) -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new().route("/metrics", get(metrics_handler));

    tokio::spawn(async move {
        let listener =
            tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
                .await
                .expect("Failed to bind metrics server");

        axum::serve(listener, app)
            .await
            .expect("Failed to start metrics server");
    });

    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("Prometheus error: {0}")]
    Prometheus(#[from] prometheus::Error),

    #[error("Server error: {0}")]
    Server(String),
}
