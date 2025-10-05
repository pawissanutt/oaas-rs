use lazy_static::lazy_static;
use opentelemetry::{
    global,
    metrics::{Counter, Histogram, Meter},
};

pub struct OdgmMetrics {
    pub object_set_total: Counter<u64>,
    pub object_get_total: Counter<u64>,
    pub normalize_latency_ms: Histogram<f64>,
    pub entry_mutations_total: Counter<u64>,
}

impl OdgmMetrics {
    fn new() -> Self {
        let meter: Meter = global::meter("oprc-odgm");
        let object_set_total = meter
            .u64_counter("odgm_object_set_total") // legacy name
            .with_description("Total object Set operations (create attempts)")
            .build();
        // Alias for future rename: odgm_objects_created_total (export same increments)
        let _alias_created = meter
            .u64_counter("odgm_objects_created_total")
            .with_description("Alias counter for created objects (same as object_set_total new objects)")
            .build();
        let object_get_total = meter
            .u64_counter("odgm_get_total")
            .with_description("Total object Get operations")
            .build();
        let normalize_latency_ms = meter
            .f64_histogram("odgm_normalize_latency_ms")
            .with_description(
                "Latency of string object id normalization in milliseconds",
            )
            .build();
        let entry_mutations_total = meter
            .u64_counter("odgm_entry_mutations_total")
            .with_description(
                "Total entry mutations (set/merge) by key variant",
            )
            .build();
        Self {
            object_set_total,
            object_get_total,
            normalize_latency_ms,
            entry_mutations_total,
        }
    }
}

lazy_static! {
    pub static ref METRICS: OdgmMetrics = OdgmMetrics::new();
}

#[inline]
pub fn incr_get(variant: &str) {
    METRICS.object_get_total.add(
        1,
        &[opentelemetry::KeyValue::new("variant", variant.to_string())],
    );
}

#[inline]
pub fn incr_set(variant: &str) {
    METRICS.object_set_total.add(
        1,
        &[opentelemetry::KeyValue::new("variant", variant.to_string())],
    );
}

#[inline]
pub fn record_normalize_latency_ms(ms: f64) {
    METRICS.normalize_latency_ms.record(ms, &[]);
}

#[inline]
pub fn incr_entry_mutation(variant: &str) {
    METRICS.entry_mutations_total.add(
        1,
        &[opentelemetry::KeyValue::new(
            "key_variant",
            variant.to_string(),
        )],
    );
}
