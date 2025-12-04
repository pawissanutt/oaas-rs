use crate::{errors::ApiError, server::AppState};
use axum::{
    Json,
    extract::{Query, State},
};
use oprc_grpc::proto::topology::{
    TopologyEdge, TopologyNode, TopologySnapshot,
};
use serde::Deserialize;
use std::collections::HashMap;
use tracing::info;

#[derive(Debug, Deserialize, Default)]
pub struct TopologyQuery {
    /// Source: "deployments" (default) or "zenoh"
    #[serde(default)]
    pub source: Option<String>,
}

#[tracing::instrument(skip(state))]
pub async fn get_topology(
    State(state): State<AppState>,
    Query(query): Query<TopologyQuery>,
) -> Result<Json<TopologySnapshot>, ApiError> {
    let source = query.source.as_deref().unwrap_or("deployments");
    info!("API: Fetching topology source={}", source);

    match source {
        "zenoh" => {
            // Get Zenoh network topology from CRM
            let topology =
                state.crm_manager.get_topology().await.map_err(|e| {
                    ApiError::InternalServerError(format!(
                        "Failed to fetch Zenoh topology: {}",
                        e
                    ))
                })?;
            Ok(Json(topology))
        }
        _ => {
            // Build deployment-based topology from PM data
            let topology = build_deployment_topology(&state).await?;
            Ok(Json(topology))
        }
    }
}

/// Build a logical topology from deployment data (packages, classes, functions, environments)
async fn build_deployment_topology(
    state: &AppState,
) -> Result<TopologySnapshot, ApiError> {
    use crate::models::DeploymentFilter;

    let deployments = state
        .deployment_service
        .list_deployments(DeploymentFilter::default())
        .await
        .map_err(|e| {
            ApiError::InternalServerError(format!(
                "Failed to list deployments: {}",
                e
            ))
        })?;

    let mut nodes: Vec<TopologyNode> = Vec::new();
    let mut edges: Vec<TopologyEdge> = Vec::new();
    let mut env_nodes: HashMap<String, bool> = HashMap::new();
    let mut package_nodes: HashMap<String, bool> = HashMap::new();

    for deployment in &deployments {
        let class_id = format!("class::{}", deployment.class_key);
        let package_id = format!("package::{}", deployment.package_name);

        // Add package node (if not already added)
        if !package_nodes.contains_key(&package_id) {
            package_nodes.insert(package_id.clone(), true);
            nodes.push(TopologyNode {
                id: package_id.clone(),
                node_type: "package".into(),
                status: "healthy".into(),
                metadata: HashMap::new(),
                deployed_classes: Vec::new(),
            });
        }

        // Add class node
        let mut class_metadata = HashMap::new();
        class_metadata
            .insert("package".into(), deployment.package_name.clone());
        if let Some(ref odgm) = deployment.odgm {
            if let Some(partitions) = odgm.partition_count {
                class_metadata
                    .insert("partitions".into(), partitions.to_string());
            }
        }
        nodes.push(TopologyNode {
            id: class_id.clone(),
            node_type: "class".into(),
            status: "healthy".into(),
            metadata: class_metadata,
            deployed_classes: vec![deployment.class_key.clone()],
        });

        // Edge: package -> class
        edges.push(TopologyEdge {
            from_id: package_id.clone(),
            to_id: class_id.clone(),
            metadata: HashMap::new(),
        });

        // Add function nodes
        for func in &deployment.functions {
            let func_id = format!(
                "function::{}::{}",
                deployment.class_key, func.function_key
            );
            let mut func_metadata = HashMap::new();
            func_metadata.insert("class".into(), deployment.class_key.clone());

            nodes.push(TopologyNode {
                id: func_id.clone(),
                node_type: "function".into(),
                status: "healthy".into(),
                metadata: func_metadata,
                deployed_classes: Vec::new(),
            });

            // Edge: class -> function
            edges.push(TopologyEdge {
                from_id: class_id.clone(),
                to_id: func_id.clone(),
                metadata: HashMap::new(),
            });
        }

        // Add environment nodes and edges
        for env in &deployment.target_envs {
            let env_id = format!("env::{}", env);

            if !env_nodes.contains_key(&env_id) {
                env_nodes.insert(env_id.clone(), true);
                let mut env_metadata = HashMap::new();
                env_metadata.insert("name".into(), env.clone());
                nodes.push(TopologyNode {
                    id: env_id.clone(),
                    node_type: "environment".into(),
                    status: "healthy".into(),
                    metadata: env_metadata,
                    deployed_classes: Vec::new(),
                });
            }

            // Edge: class -> env (deployment relationship)
            edges.push(TopologyEdge {
                from_id: class_id.clone(),
                to_id: env_id.clone(),
                metadata: HashMap::new(),
            });
        }
    }

    let now = chrono::Utc::now();
    Ok(TopologySnapshot {
        nodes,
        edges,
        timestamp: Some(oprc_grpc::Timestamp {
            seconds: now.timestamp(),
            nanos: now.timestamp_subsec_nanos() as i32,
        }),
    })
}
