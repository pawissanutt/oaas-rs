use crate::metrics::ServiceMetrics;
use std::time::Instant;

pub struct MetricsMiddleware {
    metrics: ServiceMetrics,
}

impl MetricsMiddleware {
    pub fn new(metrics: ServiceMetrics) -> Self {
        Self { metrics }
    }

    pub fn start_request(&self) -> RequestTimer {
        self.metrics.record_request();
        self.metrics.increment_active_connections();
        RequestTimer {
            start: Instant::now(),
            metrics: self.metrics.clone(),
        }
    }
}

pub struct RequestTimer {
    start: Instant,
    metrics: ServiceMetrics,
}

impl Drop for RequestTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed().as_secs_f64();
        self.metrics.record_request_duration(duration);
        self.metrics.decrement_active_connections();
    }
}

impl RequestTimer {
    pub fn record_error(&self) {
        self.metrics.record_error();
    }
}
