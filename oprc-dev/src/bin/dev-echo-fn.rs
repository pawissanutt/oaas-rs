use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use envconfig::Envconfig;
use oprc_dev::Config;
use oprc_pb::{
    oprc_function_server::{OprcFunction, OprcFunctionServer},
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
};
use tonic::{transport::Server, Request, Response, Status};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    let conf = Config::init_from_env()?;
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);
    let echo_function: OprcFunctionServer<EchoFunction> =
        OprcFunctionServer::new(EchoFunction {});
    tracing::info!("start server on port {}", conf.http_port);
    let reflection_server_v1a = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_pb::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();
    Server::builder()
        .add_service(echo_function)
        .add_service(reflection_server_v1a)
        .add_service(reflection_server_v1)
        .serve(socket)
        .await?;
    Ok(())
}

struct EchoFunction {}

#[tonic::async_trait]
impl OprcFunction for EchoFunction {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        let invocation_request = request.into_inner();
        info!("invoke_fn: {:?}", invocation_request);
        let resp = InvocationResponse {
            payload: Some(invocation_request.payload),
            // payload: None,
            status: 200,
        };
        Ok(Response::new(resp))
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let invocation_request = request.into_inner();
        info!("invoke_obj: {:?}", invocation_request);
        let resp = InvocationResponse {
            payload: Some(invocation_request.payload),
            status: 200,
        };
        Ok(Response::new(resp))
    }
}
