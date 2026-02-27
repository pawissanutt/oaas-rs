//! HTTP handlers for artifact storage and script operations.

use crate::{
    errors::ApiError,
    server::AppState,
    services::script::{BuildScriptRequest, CompileRequest, DeployScriptRequest, TestScriptRequest},
};
use axum::{
    Json,
    body::Body,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use tracing::info;

// ---------------------------------------------------------------------------
// Artifact endpoints
// ---------------------------------------------------------------------------

/// GET /api/v1/artifacts/{id}
///
/// Serves raw WASM bytes with `application/wasm` content type.
#[tracing::instrument(skip(state))]
pub async fn get_artifact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let store = state.artifact_store.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Artifact store not configured".to_string(),
        )
    })?;

    info!("API: Serving artifact: {}", id);

    let bytes = store.get(&id).await.map_err(ApiError::from)?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/wasm")
        .header(header::CONTENT_LENGTH, bytes.len().to_string())
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(bytes))
        .unwrap())
}

// ---------------------------------------------------------------------------
// Script endpoints
// ---------------------------------------------------------------------------

/// POST /api/v1/scripts/compile
///
/// Compile a script without storing — validation only.
#[tracing::instrument(skip(state, req))]
pub async fn compile_script(
    State(state): State<AppState>,
    Json(req): Json<CompileRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let script_service = state.script_service.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Script service not configured".to_string(),
        )
    })?;

    info!("API: Compiling script (language: {})", req.language);

    let response = script_service.compile_script(&req).await;
    let status = if response.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };

    Ok((status, Json(response)))
}

/// POST /api/v1/scripts/deploy
///
/// Full compile → store → deploy pipeline.
#[tracing::instrument(skip(state, req), fields(package = %req.package_name, class = %req.class_key))]
pub async fn deploy_script(
    State(state): State<AppState>,
    Json(req): Json<DeployScriptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let script_service = state.script_service.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Script service not configured".to_string(),
        )
    })?;

    info!(
        "API: Deploying script for {}/{}",
        req.package_name, req.class_key
    );

    let response = script_service.deploy_script(&req).await;
    let status = if response.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };

    Ok((status, Json(response)))
}

/// GET /api/v1/scripts/{package}/{function}
///
/// Return stored TypeScript source code for re-editing.
#[tracing::instrument(skip(state))]
pub async fn get_script_source(
    State(state): State<AppState>,
    Path((package, function)): Path<(String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    let script_service = state.script_service.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Script service not configured".to_string(),
        )
    })?;

    info!("API: Getting script source for {}/{}", package, function);

    match script_service
        .get_script_source(&package, &function)
        .await
        .map_err(ApiError::from)?
    {
        Some(response) => Ok(Json(response)),
        None => Err(ApiError::NotFound(format!(
            "Source not found for {}/{}",
            package, function
        ))),
    }
}

/// POST /api/v1/scripts/build
///
/// Compile + store artifact + store source. No package/deployment.
#[tracing::instrument(skip(state, req), fields(package = %req.package_name, class = %req.class_key))]
pub async fn build_script(
    State(state): State<AppState>,
    Json(req): Json<BuildScriptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let script_service = state.script_service.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Script service not configured".to_string(),
        )
    })?;

    info!(
        "API: Building script for {}/{}",
        req.package_name, req.class_key
    );

    let response = script_service.build_script(&req).await;
    let status = if response.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };

    Ok((status, Json(response)))
}

/// GET /api/v1/artifacts
///
/// List all stored artifacts with metadata.
#[tracing::instrument(skip(state))]
pub async fn list_artifacts(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let script_service = state.script_service.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Script service not configured".to_string(),
        )
    })?;

    info!("API: Listing artifacts");

    let entries = script_service.list_artifacts().await.map_err(ApiError::from)?;
    Ok(Json(entries))
}

/// POST /api/v1/scripts/test
///
/// Test a script method using the compiler service's real SDK runtime.
/// Ensures behavior-consistent results with WASM deployment.
#[tracing::instrument(skip(state, req), fields(method = %req.method_name))]
pub async fn test_script(
    State(state): State<AppState>,
    Json(req): Json<TestScriptRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let script_service = state.script_service.as_ref().ok_or_else(|| {
        ApiError::ServiceUnavailable(
            "Script service not configured".to_string(),
        )
    })?;

    info!("API: Testing script method: {}", req.method_name);

    let response = script_service.test_script(&req).await?;
    Ok(Json(response))
}
