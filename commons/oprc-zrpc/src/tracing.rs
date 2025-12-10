//! OpenTelemetry trace context propagation for ZRPC over Zenoh.
//!
//! This module provides utilities for injecting and extracting trace context
//! across ZRPC calls, enabling distributed tracing through Zenoh pub/sub.

#[cfg(feature = "otel")]
use opentelemetry::propagation::{Extractor, Injector};
#[cfg(feature = "otel")]
use std::collections::HashMap;

/// Carrier for OpenTelemetry context propagation via Zenoh attachment metadata.
///
/// Implements `Injector` for client-side trace context injection and `Extractor`
/// for server-side trace context extraction. Uses a simple key-value map that
/// can be serialized into Zenoh query/reply attachments.
#[cfg(feature = "otel")]
#[derive(Debug, Clone, Default)]
pub struct ZenohTraceCarrier {
    map: HashMap<String, String>,
}

#[cfg(feature = "otel")]
impl ZenohTraceCarrier {
    /// Create a new empty carrier.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create carrier from existing map (for extraction).
    pub fn from_map(map: HashMap<String, String>) -> Self {
        Self { map }
    }

    /// Get the underlying map (for serialization into Zenoh attachment).
    pub fn into_map(self) -> HashMap<String, String> {
        self.map
    }

    /// Get reference to the underlying map.
    pub fn as_map(&self) -> &HashMap<String, String> {
        &self.map
    }
}

#[cfg(feature = "otel")]
impl Injector for ZenohTraceCarrier {
    fn set(&mut self, key: &str, value: String) {
        self.map.insert(key.to_string(), value);
    }
}

#[cfg(feature = "otel")]
impl Extractor for ZenohTraceCarrier {
    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|v| v.as_str())
    }

    fn keys(&self) -> Vec<&str> {
        self.map.keys().map(|k| k.as_str()).collect()
    }
}

#[cfg(test)]
#[cfg(feature = "otel")]
mod tests {
    use super::*;

    #[test]
    fn test_carrier_injector() {
        let mut carrier = ZenohTraceCarrier::new();
        carrier.set(
            "traceparent",
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
                .to_string(),
        );
        carrier.set("tracestate", "foo=bar".to_string());

        let map = carrier.into_map();
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get("traceparent"),
            Some(
                &"00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_carrier_extractor() {
        let mut map = HashMap::new();
        map.insert(
            "traceparent".to_string(),
            "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
                .to_string(),
        );
        let carrier = ZenohTraceCarrier::from_map(map);

        assert_eq!(
            carrier.get("traceparent"),
            Some("00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01")
        );
        assert_eq!(carrier.get("missing"), None);
        assert_eq!(carrier.keys().len(), 1);
    }
}
