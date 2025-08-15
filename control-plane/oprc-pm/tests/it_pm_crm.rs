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
// (Command import removed; not used)

// Use shared test-utils env helper for setting environment variables.
use oprc_test_utils::env as test_env;

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
            provision_config: Some(oprc_models::ProvisionConfig {
                container_image: Some(
                    "ghcr.io/pawissanutt/dev-echo-fn:latest".into(),
                ),
                knative: None,
            }),
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
        odgm: None,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn inproc_deploy_smoke() -> Result<()> {
    // Env-only config for server + memory storage
    let _env = test_env::Env::new()
        .set("SERVER_HOST", "127.0.0.1")
        .set("SERVER_PORT", "0")
        .set("STORAGE_TYPE", "memory")
        .set("CRM_DEFAULT_URL", "http://127.0.0.1:8088");

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
            ..Default::default()
        };
        Ok(Response::new(GetDeploymentStatusResponse {
            status: StatusCode::Ok as i32,
            deployment: Some(dep),
            message: Some("ok".into()),
            status_resource_refs: vec![pdep::ResourceReference {
                kind: "Service".into(),
                name: "svc-a".into(),
                namespace: Some("default".into()),
                uid: Some("uid-123".into()),
            }],
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
            summarized_status: Some(pdep::SummarizedStatus::Running as i32),
            ..Default::default()
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
            ..Default::default()
        };
        Ok(Response::new(GetDeploymentRecordResponse {
            deployment: Some(dep),
            // no extra status_resource_refs here
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

    test_env::set_env("SERVER_HOST", "127.0.0.1");
    test_env::set_env("SERVER_PORT", "0");
    test_env::set_env("STORAGE_TYPE", "memory");
    let crm_url = format!("http://{}", addr);
    let _env = test_env::Env::new()
        .set("SERVER_HOST", "127.0.0.1")
        .set("SERVER_PORT", "0")
        .set("STORAGE_TYPE", "memory")
        .set("CRM_DEFAULT_URL", &crm_url);

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

    test_env::set_env("SERVER_HOST", "127.0.0.1");
    test_env::set_env("SERVER_PORT", "0");
    test_env::set_env("STORAGE_TYPE", "memory");
    let crm_url = format!("http://{}", addr);
    let _env = test_env::Env::new()
        .set("SERVER_HOST", "127.0.0.1")
        .set("SERVER_PORT", "0")
        .set("STORAGE_TYPE", "memory")
        .set("CRM_DEFAULT_URL", &crm_url);

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

#[test_log::test(tokio::test)]
#[ignore]
async fn e2e_with_kind_crm_happy_path() -> Result<()> {
    // Start CRM controller + HTTP+gRPC using runtime helpers on an ephemeral port
    let client = kube::Client::try_default().await?;
    let _controller_handle =
        oprc_crm::runtime::spawn_controller(client.clone());

    // Allocate an ephemeral port using shared util, then spawn HTTP+gRPC on it
    let addr = oprc_test_utils::net::reserve_ephemeral_port().await?;
    let _http_handle = oprc_crm::runtime::spawn_http_with_grpc(
        addr,
        client.clone(),
        "default".to_string(),
    );

    // Point PM at this CRM endpoint
    let crm_url = format!("http://{}", addr);
    let _env = test_env::Env::new()
        .set("SERVER_HOST", "127.0.0.1")
        .set("SERVER_PORT", "0")
        .set("STORAGE_TYPE", "memory")
        .set("CRM_DEFAULT_URL", &crm_url);

    let app = build_api_server_from_env().await?.into_router();

    // Create package and deployment, then read back records
    let pkg = make_test_package();
    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/packages")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&pkg)?))?,
        )
        .await?;

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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let deploy_json: serde_json::Value = serde_json::from_slice(&body)?;
    let deployment_id = deploy_json
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or(&dep.key)
        .to_string();

    // Poll until at least one deployment record shows up (controller reconciliation)
    let start = std::time::Instant::now();
    loop {
        if start.elapsed() > std::time::Duration::from_secs(5) {
            panic!("timed out waiting for deployment records");
        }
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/deployment-records")
                    .body(Body::empty())?,
            )
            .await?;
        if resp.status().is_success() {
            let body =
                axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
            let items: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
            if !items.is_empty() {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Poll deployment status until available
    let start = std::time::Instant::now();
    let status_url = format!("/api/v1/deployment-status/{}", deployment_id);
    let status_json = loop {
        if start.elapsed() > std::time::Duration::from_secs(5) {
            panic!("timed out waiting for deployment status");
        }
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(&status_url).body(Body::empty())?)
            .await?;
        if resp.status().is_success() {
            let body =
                axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
            let val: serde_json::Value = serde_json::from_slice(&body)?;
            break val;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };
    // DeploymentStatus JSON uses field "id"; previous test expected "deployment_id" which does not exist.
    assert_eq!(status_json["id"], deployment_id);

    // Delete deployment (requires cluster parameter)
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(&format!(
                    "/api/v1/deployments/{}?cluster=default",
                    deployment_id
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert!(resp.status().is_success());
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let del_json: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(del_json["id"], deployment_id);
    assert_eq!(del_json["cluster"], "default");

    // Confirm status eventually returns 404 (Not Found) after deletion (allow some propagation time)
    let start = std::time::Instant::now();
    loop {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&format!(
                        "/api/v1/deployment-status/{}",
                        deployment_id
                    ))
                    .body(Body::empty())?,
            )
            .await?;
        if resp.status() == axum::http::StatusCode::NOT_FOUND {
            break;
        }
        if start.elapsed() > std::time::Duration::from_secs(3) {
            panic!(
                "timed out waiting for deployment status 404 after deletion (last status={})",
                resp.status()
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    Ok(())
}
