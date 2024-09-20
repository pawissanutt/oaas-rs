use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use envconfig::Envconfig;
use oprc_pb::{
    oprc_function_server::{OprcFunction, OprcFunctionServer},
    routing_service_server::{RoutingService, RoutingServiceServer},
    ClsRouting, ClsRoutingRequest, ClsRoutingTable, InvocationRequest,
    InvocationResponse, ObjectInvocationRequest,
};
use tokio_stream::wrappers::ReceiverStream;
use tonic::{transport::Server, Request, Response, Status};
use tools::create_reflection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();
    let conf = tools::Config::init_from_env()?;
    let socket =
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), conf.http_port);
    let dev_pm = RoutingServiceServer::new(DevPM {});
    tracing::info!("start server on port {}", conf.http_port);
    let (v1a, v1) = create_reflection();
    Server::builder()
        .add_service(dev_pm)
        .add_service(v1a)
        .add_service(v1)
        .serve(socket)
        .await?;
    Ok(())
}

struct DevPM {}

#[tonic::async_trait]
impl RoutingService for DevPM {
    async fn get_cls_routing(
        &self,
        request: Request<ClsRoutingRequest>,
    ) -> Result<Response<ClsRoutingTable>, tonic::Status> {
        todo!()
    }

    type WatchClsRoutingStream = ReceiverStream<Result<ClsRouting, Status>>;

    async fn watch_cls_routing(
        &self,
        request: Request<ClsRoutingRequest>,
    ) -> Result<Response<Self::WatchClsRoutingStream>, tonic::Status> {
        todo!()
    }
}
