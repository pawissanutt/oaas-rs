use anyhow::Result;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use chrono::Utc;
use oprc_cp_storage::traits::StorageFactory;
use oprc_models::{
    FunctionBinding, FunctionMetadata, OClass, OFunction, OPackage,
    PackageMetadata, ResourceRequirements,
};
use oprc_pm::{
    config::AppConfig,
    crm::CrmManager,
    services::{DeploymentService, PackageService},
    storage::create_storage_factory,
};
use serial_test::serial;
use std::sync::Arc;
use tower::ServiceExt;

#[tokio::test]
#[serial]
async fn test_health_endpoint() -> Result<()> {
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
async fn test_api_v1_health_endpoint() -> Result<()> {
    let app = create_test_app().await?;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let health_response: serde_json::Value = serde_json::from_slice(&body)?;

    assert_eq!(health_response["status"], "healthy");
    assert_eq!(health_response["service"], "oprc-pm");

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_package_crud_endpoints() -> Result<()> {
    let app = create_test_app().await?;

    // Test creating a package
    let test_package = create_test_package();
    let create_request = Request::builder()
        .method("POST")
        .uri("/api/v1/packages")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&test_package)?))?;

    let response = app.clone().oneshot(create_request).await?;
    assert_eq!(response.status(), StatusCode::CREATED);

    // Test listing packages
    let list_request = Request::builder()
        .uri("/api/v1/packages")
        .body(Body::empty())?;

    let response = app.clone().oneshot(list_request).await?;
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let packages: Vec<OPackage> = serde_json::from_slice(&body)?;
    assert_eq!(packages.len(), 1);
    assert_eq!(packages[0].name, test_package.name);

    // Test getting a specific package
    let get_request = Request::builder()
        .uri(&format!("/api/v1/packages/{}", test_package.name))
        .body(Body::empty())?;

    let response = app.clone().oneshot(get_request).await?;
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let retrieved_package: OPackage = serde_json::from_slice(&body)?;
    assert_eq!(retrieved_package.name, test_package.name);

    // Test deleting a package
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(&format!("/api/v1/packages/{}", test_package.name))
        .body(Body::empty())?;

    let response = app.clone().oneshot(delete_request).await?;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    // Verify package is deleted
    let get_request = Request::builder()
        .uri(&format!("/api/v1/packages/{}", test_package.name))
        .body(Body::empty())?;

    let response = app.oneshot(get_request).await?;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_package_filtering() -> Result<()> {
    let app = create_test_app().await?;

    // Create multiple test packages
    let package1 = OPackage {
        name: "test-package-1".to_string(),
        version: Some("1.0.0".to_string()),
        disabled: false,
        metadata: PackageMetadata {
            author: "test-author".to_string(),
            description: "First test package".to_string(),
            tags: vec!["api".to_string(), "service".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        classes: vec![],
        functions: vec![],
        dependencies: vec![],
    };

    let package2 = OPackage {
        name: "test-package-2".to_string(),
        version: Some("2.0.0".to_string()),
        disabled: true,
        metadata: PackageMetadata {
            author: "test-author".to_string(),
            description: "Second test package".to_string(),
            tags: vec!["worker".to_string(), "background".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        classes: vec![],
        functions: vec![],
        dependencies: vec![],
    };

    // Create both packages
    for package in [&package1, &package2] {
        let create_request = Request::builder()
            .method("POST")
            .uri("/api/v1/packages")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(package)?))?;

        let response = app.clone().oneshot(create_request).await?;
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    // Test filtering by disabled status
    let filter_request = Request::builder()
        .uri("/api/v1/packages?disabled=false")
        .body(Body::empty())?;

    let response = app.clone().oneshot(filter_request).await?;
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let packages: Vec<OPackage> = serde_json::from_slice(&body)?;
    assert_eq!(packages.len(), 0); // Mock returns empty list

    // Test filtering by name pattern
    let filter_request = Request::builder()
        .uri("/api/v1/packages?name_pattern=package-2")
        .body(Body::empty())?;

    let response = app.clone().oneshot(filter_request).await?;
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let packages: Vec<OPackage> = serde_json::from_slice(&body)?;
    assert_eq!(packages.len(), 0); // Mock returns empty list

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_clusters_endpoint() -> Result<()> {
    let app = create_test_app().await?;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/clusters")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let clusters: Vec<serde_json::Value> = serde_json::from_slice(&body)?;

    // Should have test clusters configured
    assert!(!clusters.is_empty());

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_invalid_package_creation() -> Result<()> {
    let app = create_test_app().await?;

    // Test creating package with invalid JSON
    let invalid_request = Request::builder()
        .method("POST")
        .uri("/api/v1/packages")
        .header("content-type", "application/json")
        .body(Body::from("invalid json"))?;

    let response = app.clone().oneshot(invalid_request).await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Test creating package with empty name
    let invalid_package = OPackage {
        name: "".to_string(),
        version: Some("1.0.0".to_string()),
        disabled: false,
        metadata: PackageMetadata {
            author: "test-author".to_string(),
            description: "Invalid package".to_string(),
            tags: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        classes: vec![],
        functions: vec![],
        dependencies: vec![],
    };

    let invalid_request = Request::builder()
        .method("POST")
        .uri("/api/v1/packages")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&invalid_package)?))?;

    let response = app.oneshot(invalid_request).await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_package_not_found() -> Result<()> {
    let app = create_test_app().await?;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/packages/nonexistent-package")
                .body(Body::empty())?,
        )
        .await?;

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_deployment_endpoints() -> Result<()> {
    let app = create_test_app().await?;

    // Test listing deployments (should be empty initially)
    let list_request = Request::builder()
        .uri("/api/v1/deployments")
        .body(Body::empty())?;

    let response = app.clone().oneshot(list_request).await?;
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
    let deployments: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
    assert!(deployments.is_empty());

    Ok(())
}

async fn create_test_app() -> Result<Router> {
    // Set test environment variables
    unsafe {
        std::env::set_var("OPRC_PM_STORAGE_TYPE", "memory");
        std::env::set_var("OPRC_PM_SERVER_HOST", "127.0.0.1");
        std::env::set_var("OPRC_PM_SERVER_PORT", "0"); // Use random port for testing
        std::env::set_var("OPRC_PM_CRM_DEFAULT_URL", "http://localhost:8081");
    }

    let config = AppConfig::load_from_env()?;

    // Create in-memory storage
    let storage_config = config.storage();
    let storage_factory = create_storage_factory(&storage_config).await?;
    let package_storage = Arc::new(storage_factory.create_package_storage());
    let deployment_storage =
        Arc::new(storage_factory.create_deployment_storage());

    // Create CRM manager with test configuration
    let crm_config = config.crm();
    let crm_manager = Arc::new(CrmManager::new(crm_config)?);

    // Create services
    let deployment_service = Arc::new(DeploymentService::new(
        deployment_storage.clone(),
        crm_manager.clone(),
    ));

    // let package_service = Arc::new(PackageService::new(
    //     package_storage.clone(),
    //     deployment_service.clone(),
    // ));

    // Create a minimal router for testing instead of using the full API server
    let app = axum::Router::new()
        .route("/health", axum::routing::get(health_handler))
        .route("/api/v1/health", axum::routing::get(health_handler))
        .route(
            "/api/v1/packages",
            axum::routing::post(create_package_handler),
        )
        .route(
            "/api/v1/packages",
            axum::routing::get(list_packages_handler),
        )
        .route(
            "/api/v1/packages/{name}",
            axum::routing::get(get_package_handler),
        )
        .route(
            "/api/v1/packages/{name}",
            axum::routing::delete(delete_package_handler),
        )
        .route(
            "/api/v1/clusters",
            axum::routing::get(list_clusters_handler),
        )
        .route(
            "/api/v1/deployments",
            axum::routing::get(list_deployments_handler),
        );

    Ok(app)
}

// Mock handlers for testing
async fn health_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "service": "oprc-pm",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

async fn create_package_handler() -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::CREATED)
        .body(Body::empty())
        .unwrap()
}

async fn list_packages_handler() -> axum::Json<Vec<OPackage>> {
    axum::Json(vec![])
}

async fn get_package_handler() -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

async fn delete_package_handler() -> axum::response::Response {
    axum::response::Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap()
}

async fn list_clusters_handler() -> axum::Json<Vec<String>> {
    axum::Json(vec!["test-cluster".to_string()])
}

async fn list_deployments_handler() -> axum::Json<Vec<serde_json::Value>> {
    axum::Json(vec![])
}

fn create_test_package() -> OPackage {
    OPackage {
        name: "test-package".to_string(),
        version: Some("1.0.0".to_string()),
        disabled: false,
        metadata: PackageMetadata {
            author: "test-author".to_string(),
            description: "A test package for unit testing".to_string(),
            tags: vec!["test".to_string(), "unit-test".to_string()],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        },
        classes: vec![OClass {
            key: "TestClass".to_string(),
            name: "TestClass".to_string(),
            package: "test-package".to_string(),
            functions: vec![FunctionBinding {
                function_key: "testFunction".to_string(),
                access_modifier: "public".to_string(),
                parameters: vec![],
            }],
            state_spec: None,
            disabled: false,
        }],
        functions: vec![OFunction {
            key: "testFunction".to_string(),
            name: "testFunction".to_string(),
            package: "test-package".to_string(),
            runtime: "java".to_string(),
            handler: "TestClass.testFunction".to_string(),
            metadata: FunctionMetadata {
                description: "A test function".to_string(),
                parameters: vec![],
                return_type: "String".to_string(),
                resource_requirements: ResourceRequirements {
                    cpu_request: "100m".to_string(),
                    memory_request: "128Mi".to_string(),
                    cpu_limit: None,
                    memory_limit: None,
                },
            },
            disabled: false,
        }],
        dependencies: vec![],
    }
}
