use anyhow::Result;
use axum::{body::Body, http::Request};
use oprc_grpc::proto::health::health_service_server::{
    HealthService, HealthServiceServer,
};
use oprc_grpc::proto::health::{
    HealthCheckRequest, HealthCheckResponse, health_check_response,
};
use oprc_models::{
    DeploymentCondition, FunctionBinding, FunctionMetadata, FunctionType,
    OClass, OClassDeployment, OFunction, OPackage, PackageMetadata,
    ResourceRequirements,
};
use oprc_pm::build_api_server_from_env;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server;
use tonic::{Request as TonicRequest, Response, Status};
use tower::ServiceExt;
// Deployment gRPC server
use oprc_grpc::proto::common::StatusCode;
use oprc_grpc::proto::deployment::deployment_service_server::{
    DeploymentService, DeploymentServiceServer,
};
use oprc_grpc::proto::deployment::*;
use oprc_grpc::proto::{common as pcom, deployment as pdep};

fn set_env(k: &str, v: &str) {
    // Use unsafe like other tests in this crate to avoid lints around env mutation.
    unsafe { std::env::set_var(k, v) }
}

fn make_test_package() -> OPackage {
    OPackage {
        name: "hello-pkg".into(),
        version: Some("0.1.0".into()),
        disabled: false,
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
                immutable: false,
                parameters: vec![],
            }],
            state_spec: None,
            disabled: false,
        }],
        functions: vec![OFunction {
            key: "echo".into(),
            immutable: false,
            function_type: FunctionType::Custom,
            metadata: FunctionMetadata {
                description: Some("echo fn".into()),
                parameters: vec![],
                return_type: Some("String".into()),
                resource_requirements: ResourceRequirements {
                    cpu_request: "50m".into(),
                    memory_request: "64Mi".into(),
                    cpu_limit: None,
                    memory_limit: None,
                },
            },
            qos_requirement: None,
            qos_constraint: None,
            provision_config: None,
            disabled: false,
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
        target_env: "dev".into(),
        target_clusters: vec!["default".into()],
        nfr_requirements: oprc_models::NfrRequirements::default(),
        functions: vec![],
        condition: DeploymentCondition::Pending,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn inproc_deploy_smoke() -> Result<()> {
    // Env-only config for server + memory storage
    set_env("SERVER_HOST", "127.0.0.1");
    set_env("SERVER_PORT", "0");
    set_env("STORAGE_TYPE", "memory");
    // Provide a default CRM so deploy path can resolve a client. No real HTTP call is made by deploy().
    set_env("CRM_DEFAULT_URL", "http://127.0.0.1:8088");

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
    assert!(resp.status().is_success());

    // Create deployment
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
    assert!(resp.status().is_success());

    // Health should be available
    let resp = app
        .clone()
        .oneshot(Request::builder().uri("/health").body(Body::empty())?)
        .await?;
    assert!(resp.status().is_success());

    Ok(())
}

struct TestHealthSvc;

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
        let dep = pdep::OClassDeployment {
            key: dep_id.clone(),
            package_name: "hello-pkg".into(),
            class_key: "hello-class".into(),
            target_env: "dev".into(),
            target_clusters: vec!["default".into()],
            nfr_requirements: None,
            functions: vec![],
            created_at: Some(pcom::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
        };
        Ok(Response::new(GetDeploymentStatusResponse {
            status: StatusCode::Ok as i32,
            deployment: Some(dep),
            message: Some("ok".into()),
        }))
    }

    async fn delete_deployment(
        &self,
        _request: TonicRequest<DeleteDeploymentRequest>,
    ) -> Result<Response<DeleteDeploymentResponse>, Status> {
        Ok(Response::new(DeleteDeploymentResponse {
            status: StatusCode::Ok as i32,
            message: None,
        }))
    }

    async fn list_deployment_records(
        &self,
        _request: TonicRequest<ListDeploymentRecordsRequest>,
    ) -> Result<Response<ListDeploymentRecordsResponse>, Status> {
        let dep = pdep::OClassDeployment {
            key: "dep-hello".into(),
            package_name: "hello-pkg".into(),
            class_key: "hello-class".into(),
            target_env: "dev".into(),
            target_clusters: vec!["default".into()],
            nfr_requirements: None,
            functions: vec![],
            created_at: Some(pcom::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
        };
        Ok(Response::new(ListDeploymentRecordsResponse {
            items: vec![dep],
        }))
    }

    async fn get_deployment_record(
        &self,
        request: TonicRequest<GetDeploymentRecordRequest>,
    ) -> Result<Response<GetDeploymentRecordResponse>, Status> {
        let dep_id = request.into_inner().deployment_id;
        let dep = pdep::OClassDeployment {
            key: dep_id,
            package_name: "hello-pkg".into(),
            class_key: "hello-class".into(),
            target_env: "dev".into(),
            target_clusters: vec!["default".into()],
            nfr_requirements: None,
            functions: vec![],
            created_at: Some(pcom::Timestamp {
                seconds: chrono::Utc::now().timestamp(),
                nanos: 0,
            }),
        };
        Ok(Response::new(GetDeploymentRecordResponse {
            deployment: Some(dep),
        }))
    }
}

#[tokio::test]
async fn cluster_health_with_mock() -> Result<()> {
    // Start a gRPC Health service on an ephemeral port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(HealthServiceServer::new(TestHealthSvc))
            .serve_with_incoming(incoming)
            .await;
    });

    set_env("SERVER_HOST", "127.0.0.1");
    set_env("SERVER_PORT", "0");
    set_env("STORAGE_TYPE", "memory");
    set_env("CRM_DEFAULT_URL", &format!("http://{}", addr));

    let app = build_api_server_from_env().await?.into_router();

    // GET /api/v1/clusters should include health populated from mock
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/clusters")
                .body(Body::empty())?,
        )
        .await?;
    assert!(resp.status().is_success());
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let clusters: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
    assert!(!clusters.is_empty());
    assert_eq!(clusters[0]["health"]["status"], "Healthy");
    Ok(())
}

#[tokio::test]
async fn list_deployment_records_with_mock() -> Result<()> {
    // Start a gRPC server with Health and Deployment services
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let incoming = TcpListenerStream::new(listener);
    tokio::spawn(async move {
        let _ = Server::builder()
            .add_service(HealthServiceServer::new(TestHealthSvc))
            .add_service(DeploymentServiceServer::new(TestDeploySvc))
            .serve_with_incoming(incoming)
            .await;
    });

    // Give the server a brief moment to start listening
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    set_env("SERVER_HOST", "127.0.0.1");
    set_env("SERVER_PORT", "0");
    set_env("STORAGE_TYPE", "memory");
    set_env("CRM_DEFAULT_URL", &format!("http://{}", addr));

    let app = build_api_server_from_env().await?.into_router();

    // Small delay to allow PM to build services
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Query deployment records and ensure enum mapping is reflected
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/deployment-records")
                .body(Body::empty())?,
        )
        .await?;
    assert!(resp.status().is_success());
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let items: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
    assert!(!items.is_empty());
    // condition is enum serialized (e.g., RUNNING), phase is UNKNOWN
    assert_eq!(items[0]["status"]["condition"], "RUNNING");
    assert_eq!(items[0]["status"]["phase"], "UNKNOWN");

    Ok(())
}
