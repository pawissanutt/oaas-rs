use crate::{
    errors::ApiError,
    models::{PackageFilter, PackageResponse},
    server::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use oprc_models::OPackage;
use tracing::{error, info};

pub async fn create_package(
    State(state): State<AppState>,
    Json(package): Json<OPackage>,
) -> Result<(StatusCode, Json<PackageResponse>), ApiError> {
    info!("API: Creating package: {}", package.name);

    match state.package_service.create_package(package).await {
        Ok(package_id) => {
            let response = PackageResponse {
                id: package_id.to_string(),
                status: "created".to_string(),
                message: Some("Package created successfully".to_string()),
            };
            Ok((StatusCode::CREATED, Json(response)))
        }
        Err(e) => {
            error!("Failed to create package: {}", e);
            Err(ApiError::InternalServerError(format!(
                "Failed to create package: {}",
                e
            )))
        }
    }
}

pub async fn get_package(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<OPackage>, ApiError> {
    info!("API: Getting package: {}", name);

    match state.package_service.get_package(&name).await {
        Ok(Some(package)) => Ok(Json(package)),
        Ok(None) => {
            Err(ApiError::NotFound(format!("Package not found: {}", name)))
        }
        Err(e) => {
            error!("Failed to get package {}: {}", name, e);
            Err(ApiError::InternalServerError(format!(
                "Failed to get package: {}",
                e
            )))
        }
    }
}

pub async fn list_packages(
    State(state): State<AppState>,
    Query(filter): Query<PackageFilter>,
) -> Result<Json<Vec<OPackage>>, ApiError> {
    info!("API: Listing packages with filter: {:?}", filter);

    match state.package_service.list_packages(filter).await {
        Ok(packages) => Ok(Json(packages)),
        Err(e) => {
            error!("Failed to list packages: {}", e);
            Err(ApiError::InternalServerError(format!(
                "Failed to list packages: {}",
                e
            )))
        }
    }
}

pub async fn update_package(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(package): Json<OPackage>,
) -> Result<Json<PackageResponse>, ApiError> {
    info!("API: Updating package: {}", name);

    // Ensure the package name in the URL matches the one in the body
    if package.name != name {
        return Err(ApiError::BadRequest(
            "Package name in URL does not match package name in body"
                .to_string(),
        ));
    }

    match state.package_service.update_package(package).await {
        Ok(()) => {
            let response = PackageResponse {
                id: name,
                status: "updated".to_string(),
                message: Some("Package updated successfully".to_string()),
            };
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to update package {}: {}", name, e);
            Err(ApiError::InternalServerError(format!(
                "Failed to update package: {}",
                e
            )))
        }
    }
}

pub async fn delete_package(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    info!("API: Deleting package: {}", name);

    match state.package_service.delete_package(&name).await {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(e) => {
            error!("Failed to delete package {}: {}", name, e);
            match e {
                crate::errors::PackageManagerError::Package(
                    crate::errors::PackageError::NotFound(_),
                ) => Err(ApiError::NotFound(format!(
                    "Package not found: {}",
                    name
                ))),
                _ => Err(ApiError::InternalServerError(format!(
                    "Failed to delete package: {}",
                    e
                ))),
            }
        }
    }
}
