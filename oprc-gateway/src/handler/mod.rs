use crate::conn::ConnManager;
use crate::handler::grpc::InvocationHandler;
use crate::handler::rest::{invoke_fn, invoke_obj};
use crate::route::Routable;
use crate::rpc::RpcManager;
use axum::routing::post;
use axum::{Extension, Router};
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
    route_builder.add_service(server);
    route_builder
        .routes()
        .into_axum_router()
        .route(
            "/api/class/:cls/objects/:oid/invokes/:func",
            post(invoke_obj),
        )
        .route("/api/class/:cls/invokes/:func", post(invoke_fn))
        .layer(Extension(conn_manager))
}
