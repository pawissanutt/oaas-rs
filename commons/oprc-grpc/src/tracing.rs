//! OpenTelemetry trace context propagation for gRPC.
//!
//! This module provides middleware layers and utilities for distributed tracing
//! across gRPC service boundaries.
//!
//! # Server Side
//! Use `OtelGrpcServerLayer` to extract trace context from incoming requests
//! and create properly-parented spans:
//! ```ignore
//! Server::builder()
//!     .layer(OtelGrpcServerLayer::default())
//!     .add_service(my_service)
//!     .serve(addr).await?;
//! ```
//!
//! # Client Side  
//! Use `inject_trace_context` to inject trace context into outgoing requests:
//! ```ignore
//! let mut request = tonic::Request::new(my_message);
//! inject_trace_context(&mut request);
//! client.my_rpc(request).await?;
//! ```

// Re-export server layer for extracting trace context
pub use tonic_tracing_opentelemetry::middleware::server::OtelGrpcLayer as OtelGrpcServerLayer;

use opentelemetry::propagation::Injector;
use tonic::metadata::MetadataMap;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Injects the current OpenTelemetry trace context into a gRPC request.
///
/// This function extracts the trace context from the current tracing span
/// and injects it as W3C Trace Context headers into the gRPC request metadata.
///
/// # Example
/// ```ignore
/// let mut request = tonic::Request::new(my_message);
/// inject_trace_context(&mut request);
/// client.my_rpc(request).await?;
/// ```
pub fn inject_trace_context<T>(request: &mut tonic::Request<T>) {
    inject_trace_context_into_metadata(request.metadata_mut());
}

/// Injects the current OpenTelemetry trace context directly into gRPC metadata.
///
/// Use this when you only have access to the MetadataMap, not the full Request.
pub fn inject_trace_context_into_metadata(metadata: &mut MetadataMap) {
    let span = tracing::Span::current();
    let context = span.context();

    let mut injector = MetadataMapInjector(metadata);
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut injector);
    });
}

/// Helper struct to inject trace context into tonic MetadataMap
struct MetadataMapInjector<'a>(&'a mut MetadataMap);

impl Injector for MetadataMapInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(key) =
            tonic::metadata::MetadataKey::from_bytes(key.as_bytes())
        {
            if let Ok(val) = value.parse() {
                self.0.insert(key, val);
            }
        }
    }
}
