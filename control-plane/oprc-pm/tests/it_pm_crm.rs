use anyhow::Result;
use axum::{body::Body, http::Request};
use oprc_grpc::proto::health::health_service_server::{
    HealthService, HealthServiceServer,
};
use oprc_grpc::proto::health::{
    HealthCheckRequest, HealthCheckResponse, health_check_response,
};
use oprc_models::{
    DeploymentCondition, FunctionBinding, FunctionType, OClass,
    OClassDeployment, OFunction, OPackage, PackageMetadata,
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
use oprc_cp_storage::traits::StorageFactory;
use oprc_test_utils::env as test_env;

fn make_test_package() -> OPackage {
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
                    "ghcr.io/pawissanutt/oaas-rs/echo-fn:latest".into(),
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
        let dep = pdep::DeploymentUnit {
            id: dep_id.clone(),
            package_name: "hello-pkg".into(),
            class_key: "hello-class".into(),
            // target_cluster removed in proto
            functions: vec![],
            target_env: "dev".into(),
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

    async fn list_class_runtimes(
        &self,
        _request: TonicRequest<ListClassRuntimesRequest>,
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

    let resp = app
        .clone()
        .oneshot(Request::builder().uri("/api/v1/envs").body(Body::empty())?)
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
                .uri("/api/v1/class-runtimes")
                .body(Body::empty())?,
        )
        .await?;
    assert!(resp.status().is_success());
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await?;
    let items: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
    assert!(!items.is_empty());
    // condition is enum serialized (e.g., PENDING), phase is RUNNING
    assert_eq!(items[0]["status"]["condition"], "Running");
    assert_eq!(items[0]["status"]["phase"], "RUNNING");

    Ok(())
}

#[test_log::test(tokio::test)]
#[ignore]
async fn e2e_with_kind_crm_single() -> Result<()> {
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

#[test_log::test(tokio::test)]
#[ignore]
async fn e2e_with_kind_crm_multi() -> Result<()> {
    use oprc_cp_storage::DeploymentStorage; // bring trait into scope for get_cluster_mappings
    // Requires a kind (or any) cluster with DeploymentRecord CRD applied.
    // Spawns two CRM instances (separate HTTP+gRPC ports) and configures PM with both clusters.
    // Verifies a multi-cluster deployment creates two per-cluster deployment records and allows scoped deletion.

    let client = match kube::Client::try_default().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SKIP: no kube client: {e}");
            return Ok(());
        }
    };
    // Spawn controller (single controller reconciles CRs cluster-wide)
    let _controller_handle =
        oprc_crm::runtime::spawn_controller(client.clone());

    // Start two CRM HTTP+gRPC endpoints
    let addr_a = oprc_test_utils::net::reserve_ephemeral_port().await?; // clusterA
    let _http_a = oprc_crm::runtime::spawn_http_with_grpc(
        addr_a,
        client.clone(),
        "default".to_string(),
    );
    let addr_b = oprc_test_utils::net::reserve_ephemeral_port().await?; // clusterB logical
    let _http_b = oprc_crm::runtime::spawn_http_with_grpc(
        addr_b,
        client.clone(),
        "default".to_string(),
    );

    let cluster_a_url = format!("http://{}", addr_a);
    let cluster_b_url = format!("http://{}", addr_b);

    // Build a custom CrmManagerConfig with two clusters
    use oprc_pm::config::{
        CircuitBreakerConfig, CrmClientConfig, CrmManagerConfig,
    };
    let crm_config = CrmManagerConfig {
        clusters: std::collections::HashMap::from([
            (
                "alpha".to_string(),
                CrmClientConfig {
                    url: cluster_a_url.clone(),
                    timeout: Some(30),
                    retry_attempts: 2,
                    api_key: None,
                    tls: None,
                },
            ),
            (
                "beta".to_string(),
                CrmClientConfig {
                    url: cluster_b_url.clone(),
                    timeout: Some(30),
                    retry_attempts: 2,
                    api_key: None,
                    tls: None,
                },
            ),
        ]),
        default_cluster: Some("alpha".to_string()),
        health_check_interval: Some(30),
        circuit_breaker: Some(CircuitBreakerConfig {
            failure_threshold: 5,
            timeout: 60,
        }),
        health_cache_ttl_seconds: 5,
    };

    // Build storages from env (memory) and manual services mimicking bootstrap
    let _env = test_env::Env::new()
        .set("SERVER_HOST", "127.0.0.1")
        .set("SERVER_PORT", "0")
        .set("STORAGE_TYPE", "memory");
    let app_cfg = oprc_pm::config::AppConfig::load_from_env()?; // leverages env for storage + policy
    let storage_cfg = app_cfg.storage();
    let storage_factory =
        oprc_pm::storage::create_storage_factory(&storage_cfg).await?;
    let package_storage =
        std::sync::Arc::new(storage_factory.create_package_storage());
    let deployment_storage =
        std::sync::Arc::new(storage_factory.create_deployment_storage());

    use oprc_pm::crm::CrmManager;
    let crm_manager = std::sync::Arc::new(CrmManager::new(crm_config)?);
    let policy = app_cfg.deployment_policy();
    let deployment_service =
        std::sync::Arc::new(oprc_pm::services::DeploymentService::new(
            deployment_storage.clone(),
            crm_manager.clone(),
            policy.clone(),
        ));
    let package_service =
        std::sync::Arc::new(oprc_pm::services::PackageService::new(
            package_storage.clone(),
            deployment_service.clone(),
            policy,
        ));
    let server_cfg = app_cfg.server();
    let api = oprc_pm::server::ApiServer::new(
        package_service,
        deployment_service,
        crm_manager.clone(),
        server_cfg,
    );
    let app = api.into_router();

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

    // Multi-cluster deployment targetting alpha + beta
    let mut dep = make_test_deployment();
    dep.target_envs = vec!["alpha".into(), "beta".into()];
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
    assert!(
        resp.status().is_success(),
        "deployment creation failed: {:?}",
        resp.status()
    );

    // Retrieve per-cluster deployment id mappings from storage (wait until both saved)
    let mut alpha_unit_id: Option<String> = None;
    let mut beta_unit_id: Option<String> = None;
    let start_map = std::time::Instant::now();
    while start_map.elapsed() < std::time::Duration::from_secs(5) {
        if let Ok(m) = deployment_storage.get_cluster_mappings(&dep.key).await {
            alpha_unit_id = m.get("alpha").cloned();
            beta_unit_id = m.get("beta").cloned();
            if alpha_unit_id.is_some() && beta_unit_id.is_some() {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let alpha_unit_id =
        alpha_unit_id.expect("alpha cluster mapping not stored");
    let beta_unit_id = beta_unit_id.expect("beta cluster mapping not stored");

    // Poll list of deployment-records aggregated across clusters until we see at least 2 entries referencing different clusters
    let start = std::time::Instant::now();
    let mut saw_alpha = false;
    let mut saw_beta = false;
    while start.elapsed() < std::time::Duration::from_secs(10) {
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
            for it in &items {
                if it["cluster_name"] == "alpha" {
                    saw_alpha = true;
                }
                if it["cluster_name"] == "beta" {
                    saw_beta = true;
                }
            }
            if saw_alpha && saw_beta {
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    assert!(
        saw_alpha && saw_beta,
        "expected records from both clusters (alpha={}, beta={})",
        saw_alpha,
        saw_beta
    );
    // Sanity: found cluster presence; (ids already captured via storage mappings)

    // Delete only beta cluster deployment
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(&format!(
                    "/api/v1/deployments/{}?cluster=beta",
                    beta_unit_id
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert!(resp.status().is_success(), "cluster beta delete failed");

    // Ensure the specific beta record id disappears while alpha record remains (ignore other historical beta records)
    let start = std::time::Instant::now();
    loop {
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
            let beta_entry = items
                .iter()
                .find(|i| i["id"].as_str() == Some(&beta_unit_id));
            let alpha_still_present =
                items.iter().any(|i| i["cluster_name"] == "alpha");
            if beta_entry.is_none() {
                assert!(
                    alpha_still_present,
                    "alpha cluster record missing after beta delete"
                );
                break;
            }
            // If still present, allow it to surface as Deleted condition
            if let Some(cond) = beta_entry.and_then(|v| {
                v.get("status")
                    .and_then(|s| s.get("condition"))
                    .and_then(|c| c.as_str())
            }) {
                if cond == "Deleted" {
                    assert!(
                        alpha_still_present,
                        "alpha cluster record missing after beta delete"
                    );
                    break;
                }
            }
        }
        if start.elapsed() > std::time::Duration::from_secs(10) {
            panic!("timed out waiting for beta record removal");
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    // Finally delete alpha
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(&format!(
                    "/api/v1/deployments/{}?cluster=alpha",
                    alpha_unit_id
                ))
                .body(Body::empty())?,
        )
        .await?;
    assert!(resp.status().is_success(), "cluster alpha delete failed");

    Ok(())
}
