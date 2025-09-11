pub mod crm_info;
pub mod deployment;
pub mod health;
pub mod helpers;
pub mod builders {
    pub mod class_runtime;
}

use kube::Client;
use std::net::SocketAddr;
use tonic::service::Routes;
use tonic::transport::Server;
use tonic_reflection::server::Builder as ReflectionBuilder;
use tracing::info;

use crm_info::CrmInfoSvc;
use deployment::DeploymentSvc;
use health::HealthSvc;
use oprc_grpc::proto::deployment::deployment_service_server::DeploymentServiceServer;

pub async fn run_grpc_server(
    addr: SocketAddr,
    client: Client,
    default_namespace: String,
) -> anyhow::Result<()> {
    info!("CRM gRPC listening on {}", addr);

    let reflection = ReflectionBuilder::configure().build_v1().ok();

    let health = HealthSvc::default();
    let crm_info = CrmInfoSvc::new(client.clone(), default_namespace.clone());

    let deploy_svc = DeploymentSvc {
        client,
        default_namespace,
    };
    let tonic_deploy = DeploymentServiceServer::new(deploy_svc);

    let mut builder = Server::builder()
        .add_service(oprc_grpc::proto::health::health_service_server::HealthServiceServer::new(
            health,
        ))
        .add_service(oprc_grpc::proto::health::crm_info_service_server::CrmInfoServiceServer::new(
            crm_info,
        ))
        .add_service(tonic_deploy);

    if let Some(reflection) = reflection {
        builder = builder.add_service(reflection);
    }

    builder.serve(addr).await?;
    Ok(())
}

pub fn build_grpc_routes(client: Client, default_namespace: String) -> Routes {
    let reflection = ReflectionBuilder::configure().build_v1().ok();

    let health = HealthSvc::default();
    let crm_info = CrmInfoSvc::new(client.clone(), default_namespace.clone());

    let deploy_svc = DeploymentSvc {
        client,
        default_namespace,
    };
    let tonic_deploy = DeploymentServiceServer::new(deploy_svc);

    let mut routes = Routes::new(
        oprc_grpc::proto::health::health_service_server::HealthServiceServer::new(
            health,
        ),
    );
    routes = routes
        .add_service(oprc_grpc::proto::health::crm_info_service_server::CrmInfoServiceServer::new(
            crm_info,
        ))
        .add_service(tonic_deploy);

    if let Some(refl) = reflection {
        routes = routes.add_service(refl);
    }

    routes
}
