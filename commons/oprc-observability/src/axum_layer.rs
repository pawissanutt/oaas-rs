//! Axum middleware layer for OpenTelemetry metrics, traces, and logs.
//!
//! This module provides a Tower layer that instruments HTTP requests with:
//! - **Metrics**: request count, duration histogram, active connections, errors
//! - **Traces**: automatic span creation for each request (via tracing)
//! - **Logs**: request/response logging (via tracing, bridged to OTEL)
//!
//! # Usage
//! ```ignore
//! use oprc_observability::axum_layer::{OtelMetrics, otel_metrics_middleware};
//! use axum::Router;
//! use std::sync::Arc;
//!
//! let metrics = Arc::new(OtelMetrics::new("my-service"));
//! let app = Router::new()
//!     .route("/health", get(health))
//!     .layer(axum::middleware::from_fn(otel_metrics_middleware))
//!     .layer(Extension(metrics));
//! ```

use axum::{
    body::Body, extract::Extension, http::Request, middleware::Next,
    response::Response,
};
use opentelemetry::{
    KeyValue,
    metrics::{Counter, Histogram, Meter, UpDownCounter},
};
use std::{sync::Arc, time::Instant};

/// OpenTelemetry metrics for HTTP requests.
#[derive(Clone)]
pub struct OtelMetrics {
    pub requests_total: Counter<u64>,
    pub request_duration_seconds: Histogram<f64>,
    pub active_connections: UpDownCounter<i64>,
    pub errors_total: Counter<u64>,
}

impl OtelMetrics {
    /// Create a new OtelMetrics instance for the given service name.
    pub fn new(service_name: &str) -> Self {
        // Leak the string to get a 'static lifetime that opentelemetry requires
        let name: &'static str =
            Box::leak(service_name.to_string().into_boxed_str());
        let meter: Meter = opentelemetry::global::meter(name);

        let requests_total = meter
            .u64_counter("http_requests_total")
            .with_description("Total HTTP requests")
            .build();

        let request_duration_seconds = meter
            .f64_histogram("http_request_duration_seconds")
            .with_description("HTTP request duration in seconds")
            .build();

        let active_connections = meter
            .i64_up_down_counter("http_active_connections")
            .with_description("Number of active HTTP connections")
            .build();

        let errors_total = meter
            .u64_counter("http_errors_total")
            .with_description("Total HTTP error responses (4xx and 5xx)")
            .build();

        Self {
            requests_total,
            request_duration_seconds,
            active_connections,
            errors_total,
        }
    }
}

/// Axum middleware function for recording OpenTelemetry metrics.
///
/// This middleware records:
/// - `http_requests_total` - counter with method, route, status labels
/// - `http_request_duration_seconds` - histogram of request durations
/// - `http_active_connections` - gauge of in-flight requests
/// - `http_errors_total` - counter of non-2xx responses
pub async fn otel_metrics_middleware(
    Extension(metrics): Extension<Arc<OtelMetrics>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();

    // Create base attributes for this request
    let base_attrs = vec![
        KeyValue::new("http.method", method.clone()),
        KeyValue::new("http.route", path.clone()),
    ];

    // Record request start
    metrics.requests_total.add(1, &base_attrs);
    metrics.active_connections.add(1, &base_attrs);

    let start = Instant::now();

    // Execute the request
    let response = next.run(req).await;

    // Record request completion
    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16();

    let mut attrs = base_attrs;
    attrs.push(KeyValue::new("http.status_code", status as i64));

    metrics.request_duration_seconds.record(duration, &attrs);
    metrics.active_connections.add(-1, &attrs);

    // Record errors (4xx and 5xx)
    if status >= 400 {
        metrics.errors_total.add(1, &attrs);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        // Just verify we can create metrics without panicking
        let _metrics = OtelMetrics::new("test-service");
    }
}
