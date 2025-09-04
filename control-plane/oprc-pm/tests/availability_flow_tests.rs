use anyhow::Result;
use axum::{body::Body, http::Request};
use oprc_pm::build_api_server_from_env;
use oprc_test_utils::env as test_env;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::{Request as TonicRequest, Response, Status};
use tower::ServiceExt;

use oprc_grpc::proto::common::StatusCode;
use oprc_grpc::proto::common::Timestamp as GrpcTimestamp;
use oprc_grpc::proto::deployment::deployment_service_server::{
    DeploymentService, DeploymentServiceServer,
};
use oprc_grpc::proto::deployment::*;
use oprc_grpc::proto::health::crm_info_service_server::{
    CrmInfoService, CrmInfoServiceServer,
};
use oprc_grpc::proto::health::{
    CrmEnvHealth as CrmClusterHealth, CrmEnvRequest as CrmClusterRequest,
};

// Minimal CrmInfoService returning a fixed availability value for end-to-end verification.
struct FixedAvailabilityCrmInfoSvc {
    availability: f64,
}

#[tonic::async_trait]
impl CrmInfoService for FixedAvailabilityCrmInfoSvc {
    async fn get_env_health(
        &self,
        request: TonicRequest<CrmClusterRequest>,
    ) -> Result<Response<CrmClusterHealth>, Status> {
        let env = request.into_inner().env;
        let now = chrono::Utc::now();
        let ts = GrpcTimestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        };
        Ok(Response::new(CrmClusterHealth {
            env_name: if env.is_empty() {
                "fixed-env".into()
            } else {
                env
            },
            status: "Healthy".into(),
            crm_version: Some("test".into()),
            last_seen: Some(ts),
            node_count: Some(5),
            ready_nodes: Some(4),
            availability: Some(self.availability),
        }))
    }
}

// Simple Deployment service so that deployment requests succeed without influencing availability logic.
struct NopDeploymentSvc;
#[tonic::async_trait]
impl DeploymentService for NopDeploymentSvc {
    async fn deploy(
        &self,
        request: TonicRequest<DeployRequest>,
    ) -> Result<Response<DeployResponse>, Status> {
        let du_id = request
            .into_inner()
            .deployment_unit
            .as_ref()
            .map(|d| d.id.clone())
            .unwrap_or_else(|| "dep".into());
        Ok(Response::new(DeployResponse {
            status: StatusCode::Ok as i32,
            deployment_id: du_id,
            message: None,
        }))
    }
    async fn get_deployment_status(
        &self,
        _: TonicRequest<GetDeploymentStatusRequest>,
    ) -> Result<Response<GetDeploymentStatusResponse>, Status> {
        Ok(Response::new(GetDeploymentStatusResponse {
            status: StatusCode::Ok as i32,
            deployment: None,
            message: Some("ok".into()),
            status_resource_refs: vec![],
        }))
    }
    async fn delete_deployment(
        &self,
        _: TonicRequest<DeleteDeploymentRequest>,
    ) -> Result<Response<DeleteDeploymentResponse>, Status> {
        Ok(Response::new(DeleteDeploymentResponse {
            status: StatusCode::Ok as i32,
            message: None,
        }))
    }
    async fn list_class_runtimes(
        &self,
        _: TonicRequest<ListClassRuntimesRequest>,
    ) -> Result<Response<ListClassRuntimesResponse>, Status> {
        Ok(Response::new(ListClassRuntimesResponse { items: vec![] }))
    }
    async fn get_class_runtime(
        &self,
        _: TonicRequest<GetClassRuntimeRequest>,
    ) -> Result<Response<GetClassRuntimeResponse>, Status> {
        Ok(Response::new(GetClassRuntimeResponse { deployment: None }))
    }
}

#[tokio::test]
async fn availability_field_flows_end_to_end() -> Result<()> {
    // Start mock CRM services.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        let _ = tonic::transport::Server::builder()
            .add_service(CrmInfoServiceServer::new(
                FixedAvailabilityCrmInfoSvc { availability: 0.37 },
            ))
            .add_service(DeploymentServiceServer::new(NopDeploymentSvc))
            .serve_with_incoming(incoming)
            .await;
    });
    // Small delay so server is ready.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let crm_url = format!("http://{}", addr);
    let _env = test_env::Env::new()
        .set("SERVER_HOST", "127.0.0.1")
        .set("SERVER_PORT", "0")
        .set("STORAGE_TYPE", "memory")
        .set("CRM_DEFAULT_URL", &crm_url);

    let app = build_api_server_from_env().await?.into_router();

    // Fetch cluster listing and assert availability propagated.
    let resp = app
        .clone()
        .oneshot(Request::builder().uri("/api/v1/envs").body(Body::empty())?)
        .await?;
    assert!(resp.status().is_success());
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let clusters: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
    assert!(!clusters.is_empty(), "expected at least one cluster entry");
    let av = clusters[0]["health"]["availability"]
        .as_f64()
        .expect("availability field missing");
    assert!(
        (av - 0.37).abs() < 1e-9,
        "expected availability 0.37, got {}",
        av
    );

    Ok(())
}
