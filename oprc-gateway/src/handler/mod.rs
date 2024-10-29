use crate::handler::grpc::InvocationHandler;
use crate::handler::rest::{invoke_fn, invoke_obj};
use crate::route::Routable;
use crate::rpc::RpcManager;
use axum::routing::{get, post};
use axum::{Extension, Router};
use http::StatusCode;
use oprc_common::conn::ConnManager;
use oprc_pb::oprc_function_server::OprcFunctionServer;
use std::sync::Arc;
use tonic::service::Routes;

mod grpc;
mod rest;

pub fn build_router(
    conn_manager: Arc<ConnManager<Routable, RpcManager>>,
) -> Router {
    let server =
        OprcFunctionServer::new(InvocationHandler::new(conn_manager.clone()));
    let mut route_builder = Routes::builder();
    let reflection_server_v1a = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();
    route_builder
        .add_service(server)
        .add_service(reflection_server_v1a)
        .add_service(reflection_server_v1);
    route_builder
        .routes()
        .into_axum_router()
        .route(
            "/api/class/:cls/objects/:oid/invokes/:func",
            post(invoke_obj),
        )
        .route("/api/class/:cls/invokes/:func", post(invoke_fn))
        .route("/*path", get(no_found))
        .route("/", get(no_found))
        .layer(Extension(conn_manager))
}

use axum::response::IntoResponse;
use axum::response::Response;

pub async fn no_found() -> Result<bytes::Bytes, Response> {
    Err((StatusCode::NOT_FOUND, String::from("NOT FOUND")).into_response())
}
