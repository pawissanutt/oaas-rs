use async_trait::async_trait;
use tonic::{Request, Response, Status};
use crate::proto::package::*;

#[async_trait]
pub trait PackageServiceHandler: Send + Sync + 'static {
    async fn create_package(
        &self,
        request: Request<CreatePackageRequest>,
    ) -> Result<Response<CreatePackageResponse>, Status>;
    
    async fn get_package(
        &self,
        request: Request<GetPackageRequest>,
    ) -> Result<Response<GetPackageResponse>, Status>;
    
    async fn list_packages(
        &self,
        request: Request<ListPackagesRequest>,
    ) -> Result<Response<ListPackagesResponse>, Status>;
    
    async fn delete_package(
        &self,
        request: Request<DeletePackageRequest>,
    ) -> Result<Response<DeletePackageResponse>, Status>;
    
    async fn deploy_class(
        &self,
        request: Request<DeployClassRequest>,
    ) -> Result<Response<DeployClassResponse>, Status>;
    
    async fn report_deployment_status(
        &self,
        request: Request<ReportStatusRequest>,
    ) -> Result<Response<ReportStatusResponse>, Status>;
}

pub struct PackageServiceServer<T: PackageServiceHandler> {
    handler: T,
}

impl<T: PackageServiceHandler> PackageServiceServer<T> {
    pub fn new(handler: T) -> Self {
        Self { handler }
    }
}

#[async_trait]
impl<T: PackageServiceHandler> package_service_server::PackageService for PackageServiceServer<T> {
    async fn create_package(
        &self,
        request: Request<CreatePackageRequest>,
    ) -> Result<Response<CreatePackageResponse>, Status> {
        self.handler.create_package(request).await
    }
    
    async fn get_package(
        &self,
        request: Request<GetPackageRequest>,
    ) -> Result<Response<GetPackageResponse>, Status> {
        self.handler.get_package(request).await
    }
    
    async fn list_packages(
        &self,
        request: Request<ListPackagesRequest>,
    ) -> Result<Response<ListPackagesResponse>, Status> {
        self.handler.list_packages(request).await
    }
    
    async fn delete_package(
        &self,
        request: Request<DeletePackageRequest>,
    ) -> Result<Response<DeletePackageResponse>, Status> {
        self.handler.delete_package(request).await
    }
    
    async fn deploy_class(
        &self,
        request: Request<DeployClassRequest>,
    ) -> Result<Response<DeployClassResponse>, Status> {
        self.handler.deploy_class(request).await
    }
    
    async fn report_deployment_status(
        &self,
        request: Request<ReportStatusRequest>,
    ) -> Result<Response<ReportStatusResponse>, Status> {
        self.handler.report_deployment_status(request).await
    }
}
