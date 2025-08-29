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

    pub async fn deploy(
        &mut self,
        deployment_unit: DeploymentUnit,
    ) -> Result<DeployResponse, tonic::Status> {
        let request = tonic::Request::new(DeployRequest {
            deployment_unit: Some(deployment_unit),
        });
        let response = self.client.deploy(request).await?;
        Ok(response.into_inner())
    }

    pub async fn get_deployment_status(
        &mut self,
        deployment_id: String,
    ) -> Result<GetDeploymentStatusResponse, tonic::Status> {
        let request =
            tonic::Request::new(GetDeploymentStatusRequest { deployment_id });
        let response = self.client.get_deployment_status(request).await?;
        Ok(response.into_inner())
    }

    pub async fn delete_deployment(
        &mut self,
        deployment_id: String,
    ) -> Result<DeleteDeploymentResponse, tonic::Status> {
        let request =
            tonic::Request::new(DeleteDeploymentRequest { deployment_id });
        let response = self.client.delete_deployment(request).await?;
        Ok(response.into_inner())
    }

    pub async fn list_deployment_records(
        &mut self,
        req: ListClassRuntimesRequest,
    ) -> Result<ListClassRuntimesResponse, tonic::Status> {
        let request = tonic::Request::new(req);
        let response = self.client.list_class_runtimes(request).await?;
        Ok(response.into_inner())
    }

    pub async fn get_class_runtime(
        &mut self,
        deployment_id: String,
    ) -> Result<GetClassRuntimeResponse, tonic::Status> {
        let request =
            tonic::Request::new(GetClassRuntimeRequest { deployment_id });
        let response = self.client.get_class_runtime(request).await?;
        Ok(response.into_inner())
    }
}
