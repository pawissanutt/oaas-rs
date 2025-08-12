use thiserror::Error;

#[derive(Error, Debug)]
pub enum PackageManagerError {
    #[error("Package error: {0}")]
    Package(#[from] PackageError),

    #[error("Deployment error: {0}")]
    Deployment(#[from] DeploymentError),

    #[error("Storage error: {0}")]
    Storage(#[from] oprc_cp_storage::StorageError),

    #[error("CRM error: {0}")]
    Crm(#[from] CrmError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Validation error: {0}")]
    Validation(#[from] validator::ValidationErrors),

    #[error("Internal error: {0}")]
    Internal(String),
}

#[derive(Error, Debug)]
pub enum PackageError {
    #[error("Package not found: {0}")]
    NotFound(String),

    #[error("Package already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid package: {0}")]
    Invalid(String),

    #[error("Dependency not found: {0}")]
    DependencyNotFound(String),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),
}

#[derive(Error, Debug)]
pub enum DeploymentError {
    #[error("Deployment not found: {0}")]
    NotFound(String),

    #[error("Deployment already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid deployment: {0}")]
    Invalid(String),

    #[error("Insufficient resources: {0}")]
    InsufficientResources(String),

    #[error("Target cluster not available: {0}")]
    ClusterUnavailable(String),
}

#[derive(Error, Debug)]
pub enum CrmError {
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),

    #[error("Request failed with status: {0}")]
    RequestFailed(reqwest::StatusCode),

    #[error("JSON serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("CRM service unavailable")]
    ServiceUnavailable,

    #[error("Cluster not found: {0}")]
    ClusterNotFound(String),

    #[error("No default cluster configured")]
    NoDefaultCluster,

    #[error("Client configuration error: {0}")]
    ConfigurationError(String),
}

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Internal server error: {0}")]
    InternalServerError(String),

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        use axum::{Json, http::StatusCode};
        use serde_json::json;

        let (status, error_message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::InternalServerError(msg) => {
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
            ApiError::ServiceUnavailable(msg) => {
                (StatusCode::SERVICE_UNAVAILABLE, msg)
            }
        };

        let body = Json(json!({
            "error": error_message,
        }));

        (status, body).into_response()
    }
}
