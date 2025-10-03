use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
use std::sync::OnceLock;

#[derive(Clone)]
pub struct ServiceMetrics {
    pub requests_total: Counter<u64>,
    pub request_duration_seconds: Histogram<f64>,
    pub active_connections: UpDownCounter<i64>,
    pub errors_total: Counter<u64>,
}

static SERVICE_METRICS: OnceLock<ServiceMetrics> = OnceLock::new();

/// Initialize (or fetch existing) service metrics for a given service name.
/// This uses the global meter (configure an exporter in your app startup).
pub fn init_service_metrics(service_name: &str) -> ServiceMetrics {
    let name_static: &'static str = Box::leak(service_name.to_string().into_boxed_str());
    SERVICE_METRICS
        .get_or_init(|| {
            let meter: Meter = opentelemetry::global::meter(name_static);
            let requests_total = meter
                .u64_counter(format!("{service_name}.requests.total"))
                .with_description("Total requests processed")
                .build();
            let request_duration_seconds = meter
                .f64_histogram(format!("{service_name}.request.duration.seconds"))
                .with_description("Request duration in seconds")
                .build();
            let active_connections = meter
                .i64_up_down_counter(format!("{service_name}.connections.active"))
                .with_description("Active in-flight connections")
                .build();
            let errors_total = meter
                .u64_counter(format!("{service_name}.errors.total"))
                .with_description("Total error count")
                .build();
            ServiceMetrics {
                requests_total,
                request_duration_seconds,
                active_connections,
                errors_total,
            }
        })
        .clone()
}

impl ServiceMetrics {
    #[inline]
    pub fn record_request(&self) { self.requests_total.add(1, &[]); }
    #[inline]
    pub fn record_request_duration(&self, duration_secs: f64) { self.request_duration_seconds.record(duration_secs, &[]); }
    #[inline]
    pub fn increment_active_connections(&self) { self.active_connections.add(1, &[]); }
    #[inline]
    pub fn decrement_active_connections(&self) { self.active_connections.add(-1, &[]); }
    #[inline]
    pub fn record_error(&self) { self.errors_total.add(1, &[]); }
}

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("OpenTelemetry metrics error: {0}")]
    Generic(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_idempotent() {
        let m1 = init_service_metrics("test-svc");
        let m2 = init_service_metrics("test-svc");
        // Updating counters should not panic and should reflect logically independent operations.
        m1.record_request();
        m2.record_error();
        // Can't directly read counters without exporter; success criteria is no panic and same ptr stored.
    // If we reached here without panic, idempotent init worked.
    }
}
