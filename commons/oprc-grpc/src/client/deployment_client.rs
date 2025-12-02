use crate::proto::deployment::*;
use tonic::transport::Channel;

#[derive(Clone)]
pub struct DeploymentClient {
    client: deployment_service_client::DeploymentServiceClient<Channel>,
}

impl DeploymentClient {
    pub async fn connect(
        endpoint: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client =
            deployment_service_client::DeploymentServiceClient::connect(
                endpoint,
            )
            .await?;
        Ok(Self { client })
    }

    #[allow(unused_mut)]
    pub async fn deploy(
        &mut self,
        deployment_unit: DeploymentUnit,
    ) -> Result<DeployResponse, tonic::Status> {
        let mut request = tonic::Request::new(DeployRequest {
            deployment_unit: Some(deployment_unit),
        });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.deploy(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn get_deployment_status(
        &mut self,
        deployment_id: String,
    ) -> Result<GetDeploymentStatusResponse, tonic::Status> {
        let mut request =
            tonic::Request::new(GetDeploymentStatusRequest { deployment_id });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.get_deployment_status(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn delete_deployment(
        &mut self,
        deployment_id: String,
    ) -> Result<DeleteDeploymentResponse, tonic::Status> {
        let mut request =
            tonic::Request::new(DeleteDeploymentRequest { deployment_id });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.delete_deployment(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn list_class_runtimes(
        &mut self,
        req: ListClassRuntimesRequest,
    ) -> Result<ListClassRuntimesResponse, tonic::Status> {
        let mut request = tonic::Request::new(req);
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.list_class_runtimes(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn get_class_runtime(
        &mut self,
        deployment_id: String,
    ) -> Result<GetClassRuntimeResponse, tonic::Status> {
        let mut request =
            tonic::Request::new(GetClassRuntimeRequest { deployment_id });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.get_class_runtime(request).await?;
        Ok(response.into_inner())
    }
}
