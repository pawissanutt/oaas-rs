use async_trait::async_trait;
use oprc_grpc::server::PackageServiceHandler;
use tonic::{Request, Response, Status};

pub struct PackageSvcStub;

#[async_trait]
impl PackageServiceHandler for PackageSvcStub {
    async fn create_package(
        &self,
        _request: Request<oprc_grpc::proto::package::CreatePackageRequest>,
    ) -> Result<
        Response<oprc_grpc::proto::package::CreatePackageResponse>,
        Status,
    > {
        Err(Status::unimplemented(
            "create_package not implemented in CRM",
        ))
    }

    async fn get_package(
        &self,
        _request: Request<oprc_grpc::proto::package::GetPackageRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::GetPackageResponse>, Status>
    {
        Err(Status::unimplemented("get_package not implemented in CRM"))
    }

    async fn list_packages(
        &self,
        _request: Request<oprc_grpc::proto::package::ListPackagesRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::ListPackagesResponse>, Status>
    {
        Err(Status::unimplemented(
            "list_packages not implemented in CRM",
        ))
    }

    async fn delete_package(
        &self,
        _request: Request<oprc_grpc::proto::package::DeletePackageRequest>,
    ) -> Result<
        Response<oprc_grpc::proto::package::DeletePackageResponse>,
        Status,
    > {
        Err(Status::unimplemented(
            "delete_package not implemented in CRM",
        ))
    }

    async fn deploy_class(
        &self,
        _request: Request<oprc_grpc::proto::package::DeployClassRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::DeployClassResponse>, Status>
    {
        Err(Status::unimplemented("deploy_class not implemented in CRM"))
    }

    async fn report_deployment_status(
        &self,
        _request: Request<oprc_grpc::proto::package::ReportStatusRequest>,
    ) -> Result<Response<oprc_grpc::proto::package::ReportStatusResponse>, Status>
    {
        Err(Status::unimplemented(
            "report_deployment_status not implemented in CRM",
        ))
    }
}
