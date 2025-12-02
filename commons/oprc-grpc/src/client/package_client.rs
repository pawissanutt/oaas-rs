use crate::proto::package::*;
use tonic::transport::Channel;

#[derive(Clone)]
pub struct PackageClient {
    client: package_service_client::PackageServiceClient<Channel>,
}

impl PackageClient {
    pub async fn connect(
        endpoint: String,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let client =
            package_service_client::PackageServiceClient::connect(endpoint)
                .await?;
        Ok(Self { client })
    }

    #[allow(unused_mut)]
    pub async fn create_package(
        &mut self,
        package: OPackage,
    ) -> Result<CreatePackageResponse, tonic::Status> {
        let mut request = tonic::Request::new(CreatePackageRequest {
            package: Some(package),
        });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.create_package(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn get_package(
        &mut self,
        name: String,
    ) -> Result<GetPackageResponse, tonic::Status> {
        let mut request = tonic::Request::new(GetPackageRequest { name });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.get_package(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn list_packages(
        &mut self,
        filter: Option<String>,
        limit: u32,
        offset: u32,
    ) -> Result<ListPackagesResponse, tonic::Status> {
        let mut request = tonic::Request::new(ListPackagesRequest {
            filter,
            limit,
            offset,
        });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.list_packages(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn delete_package(
        &mut self,
        name: String,
    ) -> Result<DeletePackageResponse, tonic::Status> {
        let mut request = tonic::Request::new(DeletePackageRequest { name });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.delete_package(request).await?;
        Ok(response.into_inner())
    }

    #[allow(unused_mut)]
    pub async fn report_deployment_status(
        &mut self,
        deployment_id: String,
        status: DeploymentStatus,
    ) -> Result<ReportStatusResponse, tonic::Status> {
        let mut request = tonic::Request::new(ReportStatusRequest {
            deployment_id,
            status: Some(status),
        });
        #[cfg(feature = "otel")]
        crate::tracing::inject_trace_context(&mut request);
        let response = self.client.report_deployment_status(request).await?;
        Ok(response.into_inner())
    }
}
