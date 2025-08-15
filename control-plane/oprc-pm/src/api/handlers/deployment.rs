use crate::{
    errors::ApiError,
    models::{DeploymentFilter, DeploymentRecordFilter, DeploymentResponse},
    server::AppState,
};
use axum::{
    Json,
    extract::{Path, Query, State},
};
use oprc_models::{OClassDeployment, FunctionDeploymentSpec};
use std::collections::HashMap;
use tracing::{error, info};

pub async fn create_deployment(
    State(state): State<AppState>,
    Json(deployment): Json<OClassDeployment>,
) -> Result<Json<DeploymentResponse>, ApiError> {
    info!(
        "API: Creating deployment for package={} class={} clusters={:?}",
        deployment.package_name, deployment.class_key, deployment.target_clusters
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
            )))
        }
    };

    let class = match pkg
        .classes
        .iter()
        .find(|c| c.key == deployment.class_key)
    {
        Some(c) => c.clone(),
        None => {
            return Err(ApiError::BadRequest(format!(
                "Class '{}' not found in package '{}" ,
                deployment.class_key, pkg.name
            )))
        }
    };

    // 2) Validate/resolve target clusters via CRM manager (best-effort)
    // If validation fails due to missing clients, surface a 400.
    if let Err(e) = state
        .crm_manager
        .select_deployment_clusters(&deployment.target_clusters)
        .await
    {
        error!("Invalid target clusters {:?}: {}", deployment.target_clusters, e);
        return Err(ApiError::BadRequest("Invalid target clusters".to_string()));
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
                replicas: 1,
                resource_requirements: func.metadata.resource_requirements.clone(),
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
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    info!("API: Deleting deployment: {}", id);

    if let Some(cluster_name) = params.get("cluster") {
        match state.crm_manager.get_client(cluster_name).await {
            Ok(client) => match client.delete_deployment(&id).await {
                Ok(()) => {
                    let response = serde_json::json!({
                        "message": "Deployment deleted successfully",
                        "id": id,
                        "cluster": cluster_name
                    });
                    Ok(Json(response))
                }
                Err(e) => {
                    error!(
                        "Failed to delete deployment {} from cluster {}: {}",
                        id, cluster_name, e
                    );
                    Err(ApiError::InternalServerError(format!(
                        "Failed to delete deployment: {}",
                        e
                    )))
                }
            },
            Err(e) => {
                error!(
                    "Failed to get CRM client for cluster {}: {}",
                    cluster_name, e
                );
                Err(ApiError::BadRequest(format!(
                    "Invalid cluster: {}",
                    cluster_name
                )))
            }
        }
    } else {
        Err(ApiError::BadRequest(
            "cluster parameter is required for deployment deletion".to_string(),
        ))
    }
}

pub async fn list_deployment_records(
    State(state): State<AppState>,
    Query(filter): Query<DeploymentRecordFilter>,
) -> Result<Json<Vec<crate::models::DeploymentRecord>>, ApiError> {
    info!("API: Listing deployment records with filter: {:?}", filter);

    match state.crm_manager.get_all_deployment_records(filter).await {
        Ok(records) => Ok(Json(records)),
        Err(e) => {
            error!("Failed to list deployment records: {}", e);
            Err(ApiError::InternalServerError(format!(
                "Failed to list deployment records: {}",
                e
            )))
        }
    }
}

pub async fn get_deployment_record(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<crate::models::DeploymentRecord>, ApiError> {
    info!("API: Getting deployment record: {}", id);

    // If cluster is specified, query that specific cluster
    if let Some(cluster_name) = params.get("cluster") {
        match state.crm_manager.get_client(cluster_name).await {
            Ok(client) => match client.get_deployment_record(&id).await {
                Ok(record) => Ok(Json(record)),
                Err(e) => {
                    error!(
                        "Failed to get deployment record {} from cluster {}: {}",
                        id, cluster_name, e
                    );
                    Err(ApiError::NotFound(
                        "Deployment record not found".to_string(),
                    ))
                }
            },
            Err(e) => {
                error!(
                    "Failed to get CRM client for cluster {}: {}",
                    cluster_name, e
                );
                Err(ApiError::BadRequest(format!(
                    "Invalid cluster: {}",
                    cluster_name
                )))
            }
        }
    } else {
        // Search across all clusters
        let filter = DeploymentRecordFilter {
            limit: Some(1),
            ..Default::default()
        };
        match state.crm_manager.get_all_deployment_records(filter).await {
            Ok(records) => {
                if let Some(record) = records.into_iter().find(|r| r.id == id) {
                    Ok(Json(record))
                } else {
                    Err(ApiError::NotFound(
                        "Deployment record not found".to_string(),
                    ))
                }
            }
            Err(e) => {
                error!("Failed to search deployment records: {}", e);
                Err(ApiError::InternalServerError(format!(
                    "Failed to search deployment records: {}",
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

    // Try to get status from specific cluster if provided
    if let Some(cluster_name) = params.get("cluster") {
        match state.crm_manager.get_client(cluster_name).await {
            Ok(client) => match client.get_deployment_status(&id).await {
                Ok(status) => Ok(Json(status)),
                Err(e) => {
                    error!(
                        "Failed to get deployment status {} from cluster {}: {}",
                        id, cluster_name, e
                    );
                    Err(ApiError::NotFound(
                        "Deployment status not found".to_string(),
                    ))
                }
            },
            Err(e) => {
                error!(
                    "Failed to get CRM client for cluster {}: {}",
                    cluster_name, e
                );
                Err(ApiError::BadRequest(format!(
                    "Invalid cluster: {}",
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
