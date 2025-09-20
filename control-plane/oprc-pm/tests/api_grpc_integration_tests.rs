use anyhow::Result;
use axum::{body::Body, http::Request};
use oprc_grpc::proto::common::StatusCode;
use oprc_grpc::proto::deployment::deployment_service_server::{
    DeploymentService, DeploymentServiceServer,
};
use oprc_grpc::proto::deployment::*;
use oprc_grpc::proto::health::crm_info_service_server::{
    CrmInfoService, CrmInfoServiceServer,
};
use oprc_grpc::proto::health::health_service_server::{
    HealthService, HealthServiceServer,
};
use oprc_grpc::proto::health::{
    CrmEnvHealth as CrmClusterHealth, CrmEnvRequest as CrmClusterRequest,
    HealthCheckRequest, HealthCheckResponse, health_check_response,
};
use oprc_grpc::proto::{common as pcom, deployment as pdep};
use oprc_models::{
    DeploymentCondition, FunctionBinding, FunctionType, OClass,
    OClassDeployment, OFunction, OPackage, PackageMetadata,
};
use oprc_pm::build_api_server_from_env;
use oprc_test_utils::env as test_env;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request as TonicRequest, Response, Status};
use tower::ServiceExt;

fn make_test_package() -> OPackage {
    /* simplified: same as before */
    OPackage {
        name: "hello-pkg".into(),
        version: Some("0.1.0".into()),
        metadata: PackageMetadata {
            author: Some("it".into()),
            description: Some("integration".into()),
            tags: vec!["demo".into()],
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        },
        classes: vec![OClass {
            key: "hello-class".into(),
            description: Some("hello class".into()),
            function_bindings: vec![FunctionBinding {
                name: "echo".into(),
                function_key: "echo".into(),
                access_modifier: oprc_pm::FunctionAccessModifier::Public,
                stateless: false,
                parameters: vec![],
            }],
            state_spec: None,
        }],
        functions: vec![OFunction {
            key: "echo".into(),
            function_type: FunctionType::Custom,
            description: Some("echo fn".into()),
            provision_config: Some(oprc_models::ProvisionConfig {
                container_image: Some(
                    "ghcr.io/pawissanutt/dev-echo-fn:latest".into(),
                ),
                ..Default::default()
            }),
            config: std::collections::HashMap::new(),
        }],
        dependencies: vec![],
        deployments: vec![],
    }
}
fn make_test_deployment() -> OClassDeployment {
    let now = chrono::Utc::now();
    OClassDeployment {
        key: "dep-hello".into(),
        package_name: "hello-pkg".into(),
        class_key: "hello-class".into(),
        target_envs: vec!["default".into()],
        available_envs: vec![],
        nfr_requirements: oprc_models::NfrRequirements::default(),
        functions: vec![],
        condition: DeploymentCondition::Pending,
        odgm: None,
        status: None,
        created_at: Some(now),
        updated_at: Some(now),
    }
}

#[test_log::test(tokio::test)]
#[serial_test::serial]
async fn inproc_deploy_smoke() -> Result<()> {
    // Start a mock gRPC server implementing both Health and Deployment services
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(HealthServiceServer::new(TestHealthSvc))
            .add_service(CrmInfoServiceServer::new(TestCrmInfoSvc))
            .add_service(DeploymentServiceServer::new(TestDeploySvc))
            .serve_with_incoming(incoming)
            .await;
    });

    let crm_url = format!("http://{}", addr);
    let _env = test_env::Env::new()
        .set("SERVER_HOST", "127.0.0.1")
        .set("SERVER_PORT", "0")
        .set("STORAGE_TYPE", "memory")
        .set("CRM_DEFAULT_URL", &crm_url);

    let app = build_api_server_from_env().await?.into_router();

    // Create package
    let pkg = make_test_package();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&pkg)?))?,
        )
        .await?;
    assert!(resp.status().is_success(), "package create failed");

    // Create deployment (uses mock DeploymentService)
    let dep = make_test_deployment();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/deployments")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&dep)?))?,
        )
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body_bytes =
            axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
        panic!(
            "deployment create failed: status={} body={} ",
            status,
            String::from_utf8_lossy(&body_bytes)
        );
    }

    // Health endpoint should be alive
    let resp = app
        .clone()
        .oneshot(Request::builder().uri("/health").body(Body::empty())?)
        .await?;
    assert!(resp.status().is_success(), "health check failed");
    Ok(())
}

struct TestHealthSvc;
#[tonic::async_trait]
impl HealthService for TestHealthSvc {
    async fn check(
        &self,
        _: TonicRequest<HealthCheckRequest>,
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
        _: TonicRequest<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        Err(Status::unimplemented("watch not needed"))
    }
}

struct TestCrmInfoSvc;
#[tonic::async_trait]
impl CrmInfoService for TestCrmInfoSvc {
    async fn get_env_health(
        &self,
        _request: TonicRequest<CrmClusterRequest>,
    ) -> Result<Response<CrmClusterHealth>, Status> {
        Ok(Response::new(CrmClusterHealth {
            env_name: "default".into(),
            status: "Healthy".into(),
            last_seen: Some(pcom::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
            crm_version: None,
            node_count: Some(1),
            ready_nodes: Some(1),
            availability: Some(0.99),
        }))
    }
}
struct TestDeploySvc;
#[tonic::async_trait]
impl DeploymentService for TestDeploySvc {
    async fn deploy(
        &self,
        request: TonicRequest<DeployRequest>,
    ) -> Result<Response<DeployResponse>, Status> {
        let du_id = request
            .into_inner()
            .deployment_unit
            .as_ref()
            .map(|d| d.id.clone())
            .unwrap_or_else(|| "dep-1".to_string());
        Ok(Response::new(DeployResponse {
            status: StatusCode::Ok as i32,
            deployment_id: du_id,
            message: None,
        }))
    }
    async fn get_deployment_status(
        &self,
        request: TonicRequest<GetDeploymentStatusRequest>,
    ) -> Result<Response<GetDeploymentStatusResponse>, Status> {
        let dep_id = request.into_inner().deployment_id;
        let dep = pdep::DeploymentUnit {
            id: dep_id.clone(),
            package_name: "hello-pkg".into(),
            class_key: "hello-class".into(),
            // target_cluster removed in proto; no need to set
            functions: vec![],
            target_env: "dev".into(),
            function_bindings: vec![],
            created_at: Some(pcom::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
            odgm_config: None,
        };
        Ok(Response::new(GetDeploymentStatusResponse {
            status: StatusCode::Ok as i32,
            deployment: Some(dep),
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
        let dep = oprc_grpc::proto::runtime::ClassRuntimeSummary {
            id: "dep-hello".into(),
            deployment_unit_id: "dep-hello".into(),
            package_name: "hello-pkg".into(),
            class_key: "hello-class".into(),
            target_environment: "dev".into(),
            cluster_name: Some("dev-cluster".into()),
            status: Some(oprc_grpc::proto::runtime::ClassRuntimeStatus {
                condition:
                    oprc_grpc::proto::runtime::DeploymentCondition::Running
                        as i32,
                phase: oprc_grpc::proto::runtime::DeploymentPhase::PhaseRunning
                    as i32,
                message: Some("ok".into()),
                last_updated: chrono::Utc::now().to_rfc3339(),
                functions: vec![],
            }),
            nfr_compliance: None,
            resource_refs: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        Ok(Response::new(ListClassRuntimesResponse {
            items: vec![dep],
        }))
    }
    async fn get_class_runtime(
        &self,
        request: TonicRequest<GetClassRuntimeRequest>,
    ) -> Result<Response<GetClassRuntimeResponse>, Status> {
        let dep_id = request.into_inner().deployment_id;
        let dep = oprc_grpc::proto::runtime::ClassRuntimeSummary {
            id: dep_id.clone(),
            deployment_unit_id: dep_id,
            package_name: "hello-pkg".into(),
            class_key: "hello-class".into(),
            target_environment: "dev".into(),
            cluster_name: Some("dev-cluster".into()),
            status: Some(oprc_grpc::proto::runtime::ClassRuntimeStatus {
                condition:
                    oprc_grpc::proto::runtime::DeploymentCondition::Running
                        as i32,
                phase: oprc_grpc::proto::runtime::DeploymentPhase::PhaseRunning
                    as i32,
                message: Some("ok".into()),
                last_updated: chrono::Utc::now().to_rfc3339(),
                functions: vec![],
            }),
            nfr_compliance: None,
            resource_refs: vec![],
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        Ok(Response::new(GetClassRuntimeResponse {
            deployment: Some(dep),
        }))
    }
}
