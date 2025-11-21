use crate::handler::grpc::InvocationHandler;
use crate::handler::rest::{invoke_fn, invoke_obj};
use axum::middleware::Next;
use axum::routing::{delete, get, post, put};
use axum::{Extension, Router};
use http::StatusCode;
use oprc_grpc::data_service_server::DataServiceServer;
use oprc_grpc::oprc_function_server::OprcFunctionServer;
use oprc_invoke::proxy::ObjectProxy;
use std::time::Duration;
use tonic::service::Routes;
use tower_http::trace::TraceLayer;
// we'll extract metrics via request extensions in middleware
// use explicit type in function signature to avoid generics issues
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use std::sync::Arc;
use std::time::Instant;

mod grpc;
mod rest;

pub fn build_router(
    z_session: zenoh::Session,
    request_timeout: Duration,
) -> Router {
    let object_proxy = ObjectProxy::new(z_session);
    let server = OprcFunctionServer::new(InvocationHandler::new(
        object_proxy.clone(),
        request_timeout,
    ));
    let data_server = DataServiceServer::new(grpc::DataServiceHandler::new(
        object_proxy.clone(),
        request_timeout,
    ));
    let mut route_builder = Routes::builder();
    let reflection_server_v1a = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_grpc::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_grpc::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();
    route_builder
        .add_service(server)
        .add_service(data_server)
        .add_service(reflection_server_v1a)
        .add_service(reflection_server_v1);
    route_builder
        .routes()
        .into_axum_router()
        .route("/health", get(health))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route(
            "/api/class/{cls}/{pid}/objects/{oid}/invokes/{func}",
            post(invoke_obj),
        )
        .route("/api/class/{cls}/{pid}/invokes/{func}", post(invoke_fn))
        .route("/api/class/{cls}/{pid}/objects/{oid}", get(rest::get_obj))
        .route("/api/class/{cls}/{pid}/objects/{oid}", put(rest::put_obj))
        .route(
            "/api/class/{cls}/{pid}/objects/{oid}",
            delete(rest::del_obj),
        )
        .route("/{*path}", get(no_found))
        .route("/", get(no_found))
        .layer(Extension(object_proxy))
        .layer(Extension(request_timeout))
        .layer(Extension(0u32))
        .layer(Extension(Duration::from_millis(25)))
        .layer(TraceLayer::new_for_http())
}

use axum::response::IntoResponse;
use axum::response::Response;

pub async fn no_found() -> Result<bytes::Bytes, Response> {
    Err((StatusCode::NOT_FOUND, String::from("NOT FOUND")).into_response())
}

pub async fn health() -> Result<bytes::Bytes, Response> {
    Ok(bytes::Bytes::from_static(b"ok"))
}

pub async fn healthz() -> Result<bytes::Bytes, Response> {
    Ok(bytes::Bytes::from_static(b"ok"))
}

pub async fn readyz() -> Result<bytes::Bytes, Response> {
    Ok(bytes::Bytes::from_static(b"ready"))
}

pub struct OtelMetrics {
    pub requests_total: Counter<u64>,
    pub request_duration_seconds: Histogram<f64>,
    pub active_connections: opentelemetry::metrics::UpDownCounter<i64>,
    pub errors_total: Counter<u64>,
}

pub fn init_otel_metrics() -> OtelMetrics {
    let meter: Meter = global::meter("oprc-gateway");
    let requests_total = meter
        .u64_counter("http_requests_total")
        .with_description("Total HTTP requests")
        .init();
    let request_duration_seconds = meter
        .f64_histogram("http_request_duration_seconds")
        .with_description("HTTP request duration seconds")
        .init();
    let active_connections = meter
        .i64_up_down_counter("http_active_connections")
        .with_description("Active HTTP connections")
        .init();
    let errors_total = meter
        .u64_counter("http_errors_total")
        .with_description("Total HTTP error responses")
        .init();
    OtelMetrics {
        requests_total,
        request_duration_seconds,
        active_connections,
        errors_total,
    }
}

pub async fn otel_metrics(
    axum::extract::Extension(metrics): axum::extract::Extension<
        Arc<OtelMetrics>,
    >,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = req.method().as_str().to_string();
    let path = req.uri().path().to_string();
    let mut attrs = vec![
        opentelemetry::KeyValue::new("http.method", method),
        opentelemetry::KeyValue::new("http.route", path),
    ];
    metrics.requests_total.add(1, &attrs);
    metrics.active_connections.add(1, &attrs);
    let start = Instant::now();
    let resp = next.run(req).await;
    let dur = start.elapsed().as_secs_f64();
    attrs.push(opentelemetry::KeyValue::new(
        "http.status",
        resp.status().as_u16() as i64,
    ));
    metrics.request_duration_seconds.record(dur, &attrs);
    if !resp.status().is_success() {
        metrics.errors_total.add(1, &attrs);
    }
    metrics.active_connections.add(-1, &attrs);
    resp
}
