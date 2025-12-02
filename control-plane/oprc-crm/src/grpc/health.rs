use async_trait::async_trait;
use oprc_grpc::proto::health::{HealthCheckRequest, HealthCheckResponse};
use tonic::{Request, Response, Status};

#[derive(Default)]
pub struct HealthSvc;

#[async_trait]
impl oprc_grpc::proto::health::health_service_server::HealthService
    for HealthSvc
{
    // #[instrument(level = "debug", skip(self, _request))]
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        Ok(Response::new(HealthCheckResponse { status: 1 })) // SERVING
    }

    type WatchStream = tokio_stream::wrappers::ReceiverStream<
        Result<HealthCheckResponse, Status>,
    >;

    // #[instrument(level = "debug", skip(self, _request))]
    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let (_tx, rx) = tokio::sync::mpsc::channel(1);
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}
