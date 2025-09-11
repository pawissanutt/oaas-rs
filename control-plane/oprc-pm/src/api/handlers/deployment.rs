use crate::{
    errors::ApiError,
    models::{ClassRuntimeFilter, DeploymentFilter, DeploymentResponse},
    server::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use oprc_models::{FunctionDeploymentSpec, OClassDeployment};
use std::collections::HashMap;
use tracing::{error, info};

pub async fn create_deployment(
    State(state): State<AppState>,
    Json(deployment): Json<OClassDeployment>,
) -> Result<Json<DeploymentResponse>, ApiError> {
    info!(
        "API: Creating deployment for package={} class={} envs={:?}",
        deployment.package_name, deployment.class_key, deployment.target_envs
    );

    // 1) Resolve package and class
    let pkg = state
        .package_service
        .get_package(&deployment.package_name)
        .await
        .map_err(|e| {
            error!("Failed to load package {}: {}", deployment.package_name, e);
            ApiError::InternalServerError(format!(
                "Failed to load package: {}",
                e
            ))
        })?;

    let pkg = match pkg {
        Some(p) => p,
        None => {
            return Err(ApiError::NotFound(format!(
                "Package not found: {}",
                deployment.package_name
            )));
        }
    };

    let class = match pkg.classes.iter().find(|c| c.key == deployment.class_key)
    {
        Some(c) => c.clone(),
        None => {
            return Err(ApiError::BadRequest(format!(
                "Class '{}' not found in package '{}",
                deployment.class_key, pkg.name
            )));
        }
    };

    // 2) Validate/resolve target clusters via CRM manager (best-effort)
    // If validation fails due to missing clients, surface a 400.
    if let Err(e) = state
        .crm_manager
        .select_deployment_clusters(&deployment.target_envs)
        .await
    {
        error!("Invalid target envs {:?}: {}", deployment.target_envs, e);
        return Err(ApiError::BadRequest("Invalid target envs".to_string()));
    }

    // 3) Enrich deployment with function specs if none provided
    let mut enriched = deployment.clone();
    if enriched.functions.is_empty() {
        // Map class function_bindings -> FunctionDeploymentSpec using package functions metadata
        let mut specs: Vec<FunctionDeploymentSpec> = Vec::new();
        for binding in &class.function_bindings {
            // Find function metadata by key
            let func = pkg
                .functions
                .iter()
                .find(|f| f.key == binding.function_key)
                .ok_or_else(|| {
                    ApiError::BadRequest(format!(
                        "Function '{}' referenced by binding '{}' not found in package '{}" ,
                        binding.function_key, binding.name, pkg.name
                    ))
                })?;

            specs.push(FunctionDeploymentSpec {
                function_key: func.key.clone(),
                description: func.description.clone(),
                available_location: None,
                qos_requirement: None,
                provision_config: func.provision_config.clone(),
                config: func.config.clone(),
            });
        }
        enriched.functions = specs;
    }

    // 4) Forward to service
    match state
        .deployment_service
        .deploy_class(&class, &enriched)
        .await
    {
        Ok(deployment_id) => {
            let response = DeploymentResponse {
                id: deployment_id.to_string(),
                status: "created".to_string(),
                message: Some("Deployment created successfully".to_string()),
            };
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to create deployment: {}", e);
            Err(ApiError::InternalServerError(format!(
                "Failed to create deployment: {}",
                e
            )))
        }
    }
}

pub async fn list_deployments(
    State(state): State<AppState>,
    Query(filter): Query<DeploymentFilter>,
) -> Result<Json<Vec<OClassDeployment>>, ApiError> {
    info!("API: Listing deployments with filter: {:?}", filter);

    match state.deployment_service.list_deployments(filter).await {
        Ok(deployments) => Ok(Json(deployments)),
        Err(e) => {
            error!("Failed to list deployments: {}", e);
            Err(ApiError::InternalServerError(format!(
                "Failed to list deployments: {}",
                e
            )))
        }
    }
}

pub async fn get_deployment(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<OClassDeployment>, ApiError> {
    info!("API: Getting deployment: {}", key);

    match state.deployment_service.get_deployment(&key).await {
        Ok(Some(deployment)) => Ok(Json(deployment)),
        Ok(None) => {
            Err(ApiError::NotFound(format!("Deployment not found: {}", key)))
        }
        Err(e) => {
            error!("Failed to get deployment {}: {}", key, e);
            Err(ApiError::InternalServerError(format!(
                "Failed to get deployment: {}",
                e
            )))
        }
    }
}

pub async fn delete_deployment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("API: Deleting deployment: {}", id);

    // No env specified: fetch from storage, use selected_envs and delete across all of them
    let deployment = match state.deployment_service.get_deployment(&id).await {
        Ok(Some(dep)) => dep,
        Ok(None) => {
            return Err(ApiError::NotFound(format!(
                "Deployment not found: {}",
                id
            )));
        }
        Err(e) => {
            error!("Failed to load deployment {}: {}", id, e);
            return Err(ApiError::InternalServerError(format!(
                "Failed to load deployment: {}",
                e
            )));
        }
    };

    // Prefer runtime status.selected_envs, fallback to target_envs
    let selected_envs = if let Some(status) = &deployment.status {
        if !status.selected_envs.is_empty() {
            status.selected_envs.clone()
        } else {
            deployment.target_envs.clone()
        }
    } else {
        deployment.target_envs.clone()
    };

    // Execute multi-env deletion via service (handles cluster-id mappings and storage cleanup)
    match state.deployment_service.delete_deployment(&id).await {
        Ok(()) => {
            let response = serde_json::json!({
                "message": "Deployment deleted across environments",
                "id": id,
                "deleted_envs": selected_envs,
            });
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to delete deployment {} across envs: {}", id, e);
            Err(ApiError::InternalServerError(format!(
                "Failed to delete deployment: {}",
                e
            )))
        }
    }
}

pub async fn list_class_runtimes(
    State(state): State<AppState>,
    Query(filter): Query<ClassRuntimeFilter>,
) -> Result<Json<Vec<crate::api::views::ApiClassRuntime>>, ApiError> {
    info!("API: Listing class runtimes with filter: {:?}", filter);

    match state.crm_manager.get_all_class_runtimes(filter).await {
        Ok(items) => {
            let items: Vec<crate::api::views::ApiClassRuntime> =
                items.into_iter().map(Into::into).collect();
            Ok(Json(items))
        }
        Err(e) => {
            error!("Failed to list class runtimes: {}", e);
            Err(ApiError::InternalServerError(format!(
                "Failed to list class runtimes: {}",
                e
            )))
        }
    }
}

pub async fn get_class_runtime(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<crate::api::views::ApiClassRuntime>, ApiError> {
    info!("API: Getting class runtime: {}", id);

    // If env/cluster is specified, query that specific env
    let cluster_param =
        params.get("env").or_else(|| params.get("cluster")).cloned();
    if let Some(cluster_name) = cluster_param {
        match state.crm_manager.get_client(&cluster_name).await {
            Ok(client) => match client.get_class_runtime(&id).await {
                Ok(item) => Ok(Json(item.into())),
                Err(e) => {
                    error!(
                        "Failed to get class runtime {} from env {}: {}",
                        id, cluster_name, e
                    );
                    Err(ApiError::NotFound(
                        "Class runtime not found".to_string(),
                    ))
                }
            },
            Err(e) => {
                error!(
                    "Failed to get CRM client for env {}: {}",
                    cluster_name, e
                );
                Err(ApiError::BadRequest(format!(
                    "Invalid env: {}",
                    cluster_name
                )))
            }
        }
    } else {
        // Search across all envs
        let filter = ClassRuntimeFilter {
            limit: Some(1),
            ..Default::default()
        };
    match state.crm_manager.get_all_class_runtimes(filter).await {
            Ok(items) => {
        if let Some(item) = items.into_iter().find(|r| r.id == id) {
            Ok(Json(item.into()))
                } else {
                    Err(ApiError::NotFound(
                        "Class runtime not found".to_string(),
                    ))
                }
            }
            Err(e) => {
                error!("Failed to search class runtimes: {}", e);
                Err(ApiError::InternalServerError(format!(
                    "Failed to search class runtimes: {}",
                    e
                )))
            }
        }
    }
}

pub async fn get_deployment_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<crate::models::DeploymentStatus>, ApiError> {
    info!("API: Getting deployment status: {}", id);

    // Try to get status from specific env if provided
    let cluster_param =
        params.get("env").or_else(|| params.get("cluster")).cloned();
    if let Some(cluster_name) = cluster_param {
        match state.crm_manager.get_client(&cluster_name).await {
            Ok(client) => match client.get_deployment_status(&id).await {
                Ok(status) => Ok(Json(status)),
                Err(e) => {
                    error!(
                        "Failed to get deployment status {} from env {}: {}",
                        id, cluster_name, e
                    );
                    Err(ApiError::NotFound(
                        "Deployment status not found".to_string(),
                    ))
                }
            },
            Err(e) => {
                error!(
                    "Failed to get CRM client for env {}: {}",
                    cluster_name, e
                );
                Err(ApiError::BadRequest(format!(
                    "Invalid env: {}",
                    cluster_name
                )))
            }
        }
    } else {
        // Use default cluster
        match state.crm_manager.get_default_client().await {
            Ok(client) => match client.get_deployment_status(&id).await {
                Ok(status) => Ok(Json(status)),
                Err(e) => {
                    error!(
                        "Failed to get deployment status {} from default cluster: {}",
                        id, e
                    );
                    Err(ApiError::NotFound(
                        "Deployment status not found".to_string(),
                    ))
                }
            },
            Err(e) => {
                error!("Failed to get default CRM client: {}", e);
                Err(ApiError::ServiceUnavailable(
                    "No default cluster available".to_string(),
                ))
            }
        }
    }
}

// Debug endpoint: return cluster -> deployment unit id mappings for a logical deployment key
pub async fn get_deployment_mappings(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("API: Getting deployment cluster mappings: {}", key);
    match state.deployment_service.get_cluster_mappings(&key).await {
        Ok(map) => Ok(Json(serde_json::json!({
            "deployment_key": key,
            "cluster_ids": map,        // legacy
            "env_ids": map              // alias for env-first clients
        }))),
        Err(e) => {
            error!("Failed to get deployment mappings {}: {}", key, e);
            Err(ApiError::InternalServerError(format!(
                "Failed to get deployment mappings: {}",
                e
            )))
        }
    }
}
