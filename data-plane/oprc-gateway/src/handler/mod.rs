use crate::handler::grpc::InvocationHandler;
use crate::handler::rest::{invoke_fn, invoke_obj};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Router};
use http::StatusCode;
use oprc_grpc::data_service_server::DataServiceServer;
use oprc_grpc::oprc_function_server::OprcFunctionServer;
use oprc_invoke::proxy::ObjectProxy;
use std::time::Duration;
use tonic::service::Routes;
use tower_http::trace::TraceLayer;

// Re-export from oprc_observability for backward compatibility
pub use oprc_observability::{
    OtelMetrics, otel_metrics_middleware as otel_metrics,
};

mod grpc;
mod rest;

/// Initialize OTEL metrics for the gateway service.
pub fn init_otel_metrics() -> OtelMetrics {
    OtelMetrics::new("oprc-gateway")
}

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
