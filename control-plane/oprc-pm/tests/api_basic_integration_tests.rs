// Basic API integration tests: CRUD & simple endpoints.
use anyhow::Result;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use oprc_cp_storage::traits::StorageFactory;
use oprc_models::{
    FunctionBinding, OClass, OFunction, OPackage, PackageMetadata,
};
use oprc_pm::{FunctionAccessModifier, FunctionType};
use oprc_pm::{
    api::handlers,
    config::AppConfig,
    crm::CrmManager,
    server::AppState,
    services::{DeploymentService, PackageService},
    storage::create_storage_factory,
};
use serial_test::serial;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
#[serial]
async fn health_endpoint() -> Result<()> {
    let app = create_test_app().await?;
    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty())?)
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let health_response: serde_json::Value = serde_json::from_slice(&body)?;
    assert_eq!(health_response["status"], "healthy");
    assert_eq!(health_response["service"], "oprc-pm");
    assert!(health_response["timestamp"].is_string());
    Ok(())
}

#[tokio::test]
#[serial]
async fn package_crud_endpoints() -> Result<()> {
    let app = create_test_app().await?;
    let test_package = create_test_package();
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/v1/packages")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_package)?))?;
    let response = app.clone().oneshot(create_request).await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    let list_request = Request::builder()
        .uri("/api/v1/packages")
        .body(Body::empty())?;
    let response = app.clone().oneshot(list_request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let packages: Vec<OPackage> = serde_json::from_slice(&body)?;
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, test_package.name);
    let get_request = Request::builder()
        .uri(&format!("/api/v1/packages/{}", test_package.name))
        .body(Body::empty())?;
    let response = app.clone().oneshot(get_request).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let retrieved_package: OPackage = serde_json::from_slice(&body)?;
    assert_eq!(retrieved_package.name, test_package.name);
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/v1/packages/{}", test_package.name))
        .body(Body::empty())?;
    let response = app.clone().oneshot(delete_request).await?;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let get_request = Request::builder()
        .uri(&format!("/api/v1/packages/{}", test_package.name))
        .body(Body::empty())?;
    let response = app.oneshot(get_request).await?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    Ok(())
}

async fn create_test_app() -> Result<Router> {
    unsafe {
        std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
        std::env::set_var("OPRC_PM_SERVER_HOST", "127.0.0.1");
        std::env::set_var("OPRC_PM_SERVER_PORT", "0");
        std::env::set_var("OPRC_PM_CRM_DEFAULT_URL", "http://localhost:8081");
    }
    let config = AppConfig::load_from_env()?;
    let storage_factory = create_storage_factory(&config.storage()).await?;
    let package_storage = Arc::new(storage_factory.create_package_storage());
    let deployment_storage =
        Arc::new(storage_factory.create_deployment_storage());
    let crm_manager = Arc::new(CrmManager::new(config.crm())?);
    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage.clone(),
        crm_manager.clone(),
        oprc_pm::config::DeploymentPolicyConfig {
            max_retries: 0,
            rollback_on_partial: false,
            package_delete_cascade: false,
        },
    ));
    let package_service = Arc::new(PackageService::new(
        package_storage.clone(),
        deployment_service.clone(),
        oprc_pm::config::DeploymentPolicyConfig {
            max_retries: 0,
            rollback_on_partial: false,
            package_delete_cascade: false,
        },
    ));
    let app_state = AppState {
        package_service: package_service.clone(),
        deployment_service: deployment_service.clone(),
        crm_manager: crm_manager.clone(),
    };
    let app = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .route(
            "/api/v1/packages",
            axum::routing::post(handlers::create_package),
        )
        .route(
            "/api/v1/packages",
            axum::routing::get(handlers::list_packages),
        )
        .route(
            "/api/v1/packages/{name}",
            axum::routing::get(handlers::get_package),
        )
        .route(
            "/api/v1/packages/{name}",
            axum::routing::delete(handlers::delete_package),
        )
        .route(
            "/api/v1/clusters",
            axum::routing::get(handlers::list_clusters),
        )
        .route(
            "/api/v1/deployments",
            axum::routing::get(handlers::list_deployments),
        )
        .with_state(app_state);
    Ok(app)
}

async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "service": "oprc-pm",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

fn create_test_package() -> OPackage {
    OPackage {
        name: "test-package".to_string(),
        version: Some("1.0.0".to_string()),
        disabled: false,
        metadata: PackageMetadata {
            author: Some("test-author".to_string()),
            description: Some("A test package for unit testing".to_string()),
            tags: vec!["test".to_string(), "unit-test".to_string()],
            created_at: Some(Utc::now()),
            updated_at: Some(Utc::now()),
        },
        classes: vec![OClass {
            key: "TestClass".to_string(),
            description: Some("A test class".to_string()),
            function_bindings: vec![FunctionBinding {
                name: "testFunction".to_string(),
                function_key: "testFunction".to_string(),
                access_modifier: FunctionAccessModifier::Public,
                immutable: false,
                parameters: vec![],
            }],
            state_spec: None,
            disabled: false,
        }],
        functions: vec![OFunction {
            key: "testFunction".to_string(),
            function_type: FunctionType::Custom,
            description: Some("A test function".to_string()),
            provision_config: None,
            config: std::collections::HashMap::new(),
        }],
        dependencies: vec![],
        deployments: vec![],
    }
}
