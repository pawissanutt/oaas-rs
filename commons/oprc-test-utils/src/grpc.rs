use oprc_grpc::proto::health::health_service_server::{
    HealthService, HealthServiceServer,
};
use oprc_grpc::proto::health::{
    HealthCheckRequest, HealthCheckResponse, health_check_response,
};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request as TonicRequest, Response, Status};

pub struct TestHealthSvc;
#[tonic::async_trait]
impl HealthService for TestHealthSvc {
    async fn check(
        &self,
        _request: TonicRequest<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse {
            status: health_check_response::ServingStatus::Serving as i32,
        }))
    }
    type WatchStream = tokio_stream::wrappers::ReceiverStream<
        Result<HealthCheckResponse, Status>,
    >;
    async fn watch(
        &self,
        _request: TonicRequest<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        let _ = tx
            .send(Ok(HealthCheckResponse {
                status: health_check_response::ServingStatus::Serving as i32,
            }))
            .await;
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}

pub async fn spawn_health_server()
-> anyhow::Result<(String, tokio::task::JoinHandle<()>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let incoming = TcpListenerStream::new(listener);
    let handle = tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(HealthServiceServer::new(TestHealthSvc))
            .serve_with_incoming(incoming)
            .await;
    });
    Ok((format!("http://{}", addr), handle))
}
