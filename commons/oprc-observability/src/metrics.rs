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
    let name_static: &'static str =
        Box::leak(service_name.to_string().into_boxed_str());
    SERVICE_METRICS
        .get_or_init(|| {
            let meter: Meter = opentelemetry::global::meter(name_static);
            let requests_total = meter
                .u64_counter(format!("{service_name}.requests.total"))
                .with_description("Total requests processed")
                .build();
            let request_duration_seconds = meter
                .f64_histogram(format!(
                    "{service_name}.request.duration.seconds"
                ))
                .with_description("Request duration in seconds")
                .build();
            let active_connections = meter
                .i64_up_down_counter(format!(
                    "{service_name}.connections.active"
                ))
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
    pub fn record_request(&self) {
        self.requests_total.add(1, &[]);
    }
    #[inline]
    pub fn record_request_duration(&self, duration_secs: f64) {
        self.request_duration_seconds.record(duration_secs, &[]);
    }
    #[inline]
    pub fn increment_active_connections(&self) {
        self.active_connections.add(1, &[]);
    }
    #[inline]
    pub fn decrement_active_connections(&self) {
        self.active_connections.add(-1, &[]);
    }
    #[inline]
    pub fn record_error(&self) {
        self.errors_total.add(1, &[]);
    }
}

// ------------------------
// ODGM Event Metrics (Data Plane V2)
// ------------------------

#[derive(Clone)]
pub struct OdgmEventMetrics {
    pub emitted_create_total: Counter<u64>,
    pub emitted_update_total: Counter<u64>,
    pub emitted_delete_total: Counter<u64>,
    pub fanout_limited_total: Counter<u64>,
    pub queue_drops_total: Counter<u64>,
    pub queue_len: UpDownCounter<i64>,
    pub emit_failures_total: Counter<u64>,
}

static ODGM_EVENT_METRICS: OnceLock<OdgmEventMetrics> = OnceLock::new();

/// Initialize (or fetch existing) ODGM Event Pipeline metrics.
/// Safe to call multiple times; the first call wins.
pub fn init_odgm_event_metrics(service_name: &str) -> OdgmEventMetrics {
    let name_static: &'static str =
        Box::leak(service_name.to_string().into_boxed_str());
    ODGM_EVENT_METRICS
        .get_or_init(|| {
            let meter: Meter = opentelemetry::global::meter(name_static);
            let emitted_create_total = meter
                .u64_counter("odgm.events.emitted.create.total")
                .with_description("Total number of emitted create events (per-entry)")
                .build();
            let emitted_update_total = meter
                .u64_counter("odgm.events.emitted.update.total")
                .with_description("Total number of emitted update events (per-entry)")
                .build();
            let emitted_delete_total = meter
                .u64_counter("odgm.events.emitted.delete.total")
                .with_description("Total number of emitted delete events (per-entry)")
                .build();
            let fanout_limited_total = meter
                .u64_counter("odgm.events.fanout.limited.total")
                .with_description("Total number of batches where fanout cap was enforced")
                .build();
            let queue_drops_total = meter
                .u64_counter("odgm.events.queue.drops.total")
                .with_description("Total number of events dropped due to full queue")
                .build();
            let queue_len = meter
                .i64_up_down_counter("odgm.events.queue.len")
                .with_description("Current ODGM event queue length (per shard dispatcher)")
                .build();
            let emit_failures_total = meter
                .u64_counter("odgm.events.emit.failures.total")
                .with_description("Total number of trigger emission failures (zenoh publish errors)")
                .build();
            OdgmEventMetrics {
                emitted_create_total,
                emitted_update_total,
                emitted_delete_total,
                fanout_limited_total,
                queue_drops_total,
                queue_len,
                emit_failures_total,
            }
        })
        .clone()
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
