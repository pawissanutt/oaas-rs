use crate::{
    errors::ApiError,
    models::{ClusterInfo, ClusterHealth},
    server::AppState,
};
use axum::{
    extract::{Path, State},
    Json,
};
use tracing::{info, error};
use chrono::Utc;

pub async fn list_clusters(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClusterInfo>>, ApiError> {
    info!("API: Listing clusters");

    let cluster_names = state.crm_manager.list_clusters().await;
    let mut cluster_infos = Vec::new();

    for cluster_name in cluster_names {
        let health = match state.crm_manager.get_cluster_health(&cluster_name).await {
            Ok(health) => health,
            Err(e) => {
                error!("Failed to get health for cluster {}: {}", cluster_name, e);
                ClusterHealth {
                    cluster_name: cluster_name.clone(),
                    status: "Unknown".to_string(),
                    crm_version: None,
                    last_seen: Utc::now(),
                    node_count: None,
                    ready_nodes: None,
                }
            },
        };

        cluster_infos.push(ClusterInfo {
            name: cluster_name,
            health,
        });
    }

    Ok(Json(cluster_infos))
}

pub async fn list_clusters_health(
    State(state): State<AppState>,
) -> Result<Json<Vec<ClusterHealth>>, ApiError> {
    info!("API: Listing cluster health");

    let cluster_names = state.crm_manager.list_clusters().await;
    let mut list = Vec::new();
    for cluster_name in cluster_names {
        let health = match state.crm_manager.get_cluster_health(&cluster_name).await {
            Ok(h) => h,
            Err(e) => {
                error!("Failed to get health for cluster {}: {}", cluster_name, e);
                ClusterHealth {
                    cluster_name: cluster_name.clone(),
                    status: "Unknown".to_string(),
                    crm_version: None,
                    last_seen: Utc::now(),
                    node_count: None,
                    ready_nodes: None,
                }
            }
        };
        list.push(health);
    }
    Ok(Json(list))
}

pub async fn get_cluster_health(
    State(state): State<AppState>,
    Path(cluster_name): Path<String>,
) -> Result<Json<ClusterHealth>, ApiError> {
    info!("API: Getting health for cluster: {}", cluster_name);

    match state.crm_manager.get_cluster_health(&cluster_name).await {
        Ok(health) => Ok(Json(health)),
        Err(e) => {
            error!("Failed to get health for cluster {}: {}", cluster_name, e);
            match e {
                crate::errors::CrmError::ClusterNotFound(_) => {
                    Err(ApiError::NotFound(format!("Cluster not found: {}", cluster_name)))
                }
                _ => {
                    Err(ApiError::InternalServerError(format!("Failed to get cluster health: {}", e)))
                }
            }
        }
    }
}

pub async fn list_classes(
    State(state): State<AppState>,
) -> Result<Json<Vec<oprc_models::OClass>>, ApiError> {
    info!("API: Listing classes across packages");

    // Get all packages and extract classes
    let filter = crate::models::PackageFilter::default();
    match state.package_service.list_packages(filter).await {
        Ok(packages) => {
            let mut all_classes = Vec::new();
            for package in packages {
                all_classes.extend(package.classes);
            }
            Ok(Json(all_classes))
        }
        Err(e) => {
            error!("Failed to list classes: {}", e);
            Err(ApiError::InternalServerError(format!("Failed to list classes: {}", e)))
        }
    }
}

pub async fn list_functions(
    State(state): State<AppState>,
) -> Result<Json<Vec<oprc_models::OFunction>>, ApiError> {
    info!("API: Listing functions across packages");

    // Get all packages and extract functions
    let filter = crate::models::PackageFilter::default();
    match state.package_service.list_packages(filter).await {
        Ok(packages) => {
            let mut all_functions = Vec::new();
            for package in packages {
                all_functions.extend(package.functions);
            }
            Ok(Json(all_functions))
        }
        Err(e) => {
            error!("Failed to list functions: {}", e);
            Err(ApiError::InternalServerError(format!("Failed to list functions: {}", e)))
        }
    }
}
