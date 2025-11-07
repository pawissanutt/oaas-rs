use opentelemetry::metrics::{Counter, Meter};
use std::sync::OnceLock;

struct GatewayMetrics {
    attempt: Counter<u64>,
    rejected: Counter<u64>,
}

static METRICS: OnceLock<GatewayMetrics> = OnceLock::new();

fn get_metrics() -> &'static GatewayMetrics {
    METRICS.get_or_init(|| {
        let meter: Meter = opentelemetry::global::meter("oprc-gateway");
        let attempt = meter
            .u64_counter("gateway.string_id.attempt")
            .with_description(
                "Number of requests that attempted to use a string object id",
            )
            .build();
        let rejected = meter
            .u64_counter("gateway.string_id.rejected")
            .with_description(
                "Number of string object id requests rejected by validation",
            )
            .build();
        GatewayMetrics { attempt, rejected }
    })
}

pub fn inc_attempt() {
    let m = get_metrics();
    m.attempt.add(1, &[]);
}

pub fn inc_rejected() {
    let m = get_metrics();
    m.rejected.add(1, &[]);
}
