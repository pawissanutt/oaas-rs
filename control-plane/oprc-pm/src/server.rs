use crate::{
    api::{create_middleware_stack, handlers},
    config::ServerConfig,
    crm::CrmManager,
    services::{DeploymentService, PackageService},
};
use axum::{
    Router,
    routing::{delete, get, post},
};
use std::{net::SocketAddr, sync::Arc};
use tracing::info;

#[derive(Clone)]
pub struct AppState {
    pub package_service: Arc<PackageService>,
    pub deployment_service: Arc<DeploymentService>,
    pub crm_manager: Arc<CrmManager>,
}

pub struct ApiServer {
    app: Router,
    config: ServerConfig,
}

impl ApiServer {
    pub fn new(
        package_service: Arc<PackageService>,
        deployment_service: Arc<DeploymentService>,
        crm_manager: Arc<CrmManager>,
        config: ServerConfig,
    ) -> Self {
        let state = AppState {
            package_service,
            deployment_service,
            crm_manager,
        };

        let app = Router::new()
            // Package Management APIs
            .route("/api/v1/packages", post(handlers::create_package))
            .route("/api/v1/packages", get(handlers::list_packages))
            .route("/api/v1/packages/{name}", get(handlers::get_package))
            .route("/api/v1/packages/{name}", post(handlers::update_package))
            .route("/api/v1/packages/{name}", delete(handlers::delete_package))
            // Deployment Management APIs
            .route("/api/v1/deployments", get(handlers::list_deployments))
            .route("/api/v1/deployments", post(handlers::create_deployment))
            .route("/api/v1/deployments/{key}", get(handlers::get_deployment))
            .route(
                "/api/v1/deployments/{key}",
                delete(handlers::delete_deployment),
            )
            // Deployment Record APIs (from CRM clusters)
            .route(
                "/api/v1/deployment-records",
                get(handlers::list_deployment_records),
            )
            .route(
                "/api/v1/deployment-records/{id}",
                get(handlers::get_deployment_record),
            )
            .route(
                "/api/v1/deployment-status/{id}",
                get(handlers::get_deployment_status),
            )
            // Multi-cluster Management APIs
            .route("/api/v1/clusters", get(handlers::list_clusters))
            .route(
                "/api/v1/clusters/health",
                get(handlers::list_clusters_health),
            )
            .route(
                "/api/v1/clusters/{name}/health",
                get(handlers::get_cluster_health),
            )
            // Class and Function APIs
            .route("/api/v1/classes", get(handlers::list_classes))
            .route("/api/v1/functions", get(handlers::list_functions))
            // Health check endpoint
            .route("/health", get(health_check))
            // Add middleware
            .layer(create_middleware_stack())
            .with_state(state);

        Self { app, config }
    }

    pub async fn serve(self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.config.port));
        let listener = tokio::net::TcpListener::bind(addr).await?;

        info!("Package Manager API server listening on {}", addr);
        info!("Health check available at: http://{}/health", addr);
        info!("API documentation: http://{}/api/v1", addr);

        axum::serve(listener, self.app).await?;

        Ok(())
    }

    /// Consume and return the underlying Axum Router so callers can serve it themselves
    /// (e.g., on an ephemeral port in tests) and discover the bound address.
    pub fn into_router(self) -> Router {
        self.app
    }
}

async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "healthy",
        "service": "oprc-pm",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}
