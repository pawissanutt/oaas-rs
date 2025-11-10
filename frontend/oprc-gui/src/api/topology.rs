#[cfg(feature = "server")]
use crate::types::TopologySource;
use crate::types::{TopologyRequest, TopologySnapshot};
use dioxus::prelude::*;
#[cfg(feature = "server")]
use oprc_models::DeploymentCondition;

// ─────────────────────────────────────────────────────────────────────
// Class Runtime DTOs (hoisted to module scope so fetch function return type resolves)
// ─────────────────────────────────────────────────────────────────────
#[cfg(feature = "server")]
#[derive(serde::Deserialize, Debug, Clone)]
struct ApiClassRuntimeStatus {
    condition: String,
    phase: String,
    message: Option<String>,
    last_updated: String,
    #[allow(dead_code)]
    functions: Option<Vec<serde_json::Value>>, // not displayed yet
}

#[cfg(feature = "server")]
#[derive(serde::Deserialize, Debug, Clone)]
struct ApiClassRuntime {
    id: String,
    deployment_unit_id: String,
    package_name: String,
    class_key: String,
    target_environment: String,
    cluster_name: Option<String>,
    status: Option<ApiClassRuntimeStatus>,
    created_at: String,
    updated_at: String,
}

#[post("/api/proxy/topology")]
pub async fn proxy_topology(
    req: TopologyRequest,
) -> Result<TopologySnapshot, ServerFnError> {
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }
    #[cfg(feature = "server")]
    {
        // Serve mock topology only when source is Zenoh; otherwise use real deployments topology.
        match req.source {
            TopologySource::Zenoh => {
                Ok(crate::api::mock::mock_topology_snapshot())
            }
            TopologySource::Deployments => topology_from_deployments().await,
        }
    }
}

#[cfg(feature = "server")]
async fn topology_from_deployments() -> Result<TopologySnapshot, ServerFnError>
{
    use crate::api::deployments;
    use crate::types::TopologyNode;
    use chrono::Utc;
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    let deployments = deployments::proxy_deployments().await?;
    // Build quick lookup maps for deployments
    let mut dep_id_by_pkg_class: HashMap<(String, String), String> =
        HashMap::new();
    // More precise: index by (package, class, env) to match runtime target env
    let mut dep_id_by_pkg_class_env: HashMap<(String, String, String), String> =
        HashMap::new();

    // Seed environment nodes from PM /api/v1/envs so we always show actual envs
    let mut env_nodes: BTreeMap<String, TopologyNode> =
        build_env_nodes_from_pm().await?;
    let mut env_counts: HashMap<String, usize> = HashMap::new();
    let mut deployment_nodes: Vec<TopologyNode> = Vec::new();
    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();

    for deployment in deployments {
        let envs = derive_environments(&deployment);
        let status_label = condition_to_status(&deployment.condition);
        let deployment_id = format!("deploy::{}", deployment.key);

        dep_id_by_pkg_class.insert(
            (
                deployment.package_name.clone(),
                deployment.class_key.clone(),
            ),
            deployment_id.clone(),
        );
        // Index by (package, class, env) for precise matching
        for env in envs.iter() {
            dep_id_by_pkg_class_env.insert(
                (
                    deployment.package_name.clone(),
                    deployment.class_key.clone(),
                    env.clone(),
                ),
                deployment_id.clone(),
            );
        }

        for env in envs.iter() {
            let entry = env_nodes.entry(env.clone()).or_insert_with(|| {
                let mut metadata = HashMap::new();
                metadata.insert("environment".into(), env.clone());
                metadata.insert("source".into(), "pm-deployments".into());
                TopologyNode {
                    id: format!("env::{}", env),
                    node_type: "environment".into(),
                    status: "healthy".into(),
                    metadata,
                    deployed_classes: Vec::new(),
                }
            });

            if !entry.deployed_classes.contains(&deployment.class_key) {
                entry.deployed_classes.push(deployment.class_key.clone());
            }

            // Do NOT override environment status based on a single deployment condition.
            // Environment health should reflect PM-reported cluster health only.

            *env_counts.entry(env.clone()).or_insert(0) += 1;
            // Reverse direction: deployment -> environment
            edges.insert((deployment_id.clone(), entry.id.clone()));
        }

        let mut metadata = HashMap::new();
        metadata.insert("package".into(), deployment.package_name.clone());
        metadata.insert("class".into(), deployment.class_key.clone());
        metadata.insert(
            "condition".into(),
            format!("{:?}", deployment.condition).to_lowercase(),
        );
        if !deployment.target_envs.is_empty() {
            metadata.insert(
                "target_envs".into(),
                deployment.target_envs.join(", "),
            );
        }
        if !deployment.available_envs.is_empty() {
            metadata.insert(
                "available_envs".into(),
                deployment.available_envs.join(", "),
            );
        }
        if let Some(status) = &deployment.status {
            if !status.selected_envs.is_empty() {
                metadata.insert(
                    "selected_envs".into(),
                    status.selected_envs.join(", "),
                );
            }
            metadata.insert(
                "replication_factor".into(),
                status.replication_factor.to_string(),
            );
            if let Some(availability) = status.achieved_quorum_availability {
                metadata.insert(
                    "achieved_quorum_availability".into(),
                    format!("{availability:.3}"),
                );
            }
            if let Some(last_error) = &status.last_error {
                metadata.insert("last_error".into(), last_error.clone());
            }
        }

        if !deployment.functions.is_empty() {
            let func_keys: Vec<_> = deployment
                .functions
                .iter()
                .map(|f| f.function_key.clone())
                .collect();
            metadata.insert("functions".into(), func_keys.join(", "));
        }

        let mut deployed_classes = vec![deployment.class_key.clone()];
        deployed_classes.sort();
        deployed_classes.dedup();

        deployment_nodes.push(TopologyNode {
            id: deployment_id,
            node_type: "function".into(),
            status: status_label.to_string(),
            metadata,
            deployed_classes,
        });
    }

    // ─────────────────────────────────────────────────────────────────────
    // Fetch Class Runtimes from PM and attach as nodes
    // ─────────────────────────────────────────────────────────────────────
    let runtimes = fetch_class_runtimes().await.unwrap_or_default();
    let mut runtime_nodes: Vec<TopologyNode> = Vec::new();

    for rt in runtimes {
        let runtime_id = format!("rt::{}", rt.id);

        // Determine status
        let status = rt
            .status
            .as_ref()
            .map(|s| s.condition.as_str())
            .map(|c| match c {
                "Running" => "healthy",
                "Deploying" | "Pending" => "degraded",
                "Down" | "Deleted" => "down",
                _ => "degraded",
            })
            .unwrap_or("degraded");

        // Ensure environment node exists (if PM /envs didn’t include, create a minimal one)
        let env_name = rt.target_environment.clone();
        let env_node_id = format!("env::{}", env_name);
        let entry = env_nodes.entry(env_name.clone()).or_insert_with(|| {
            let mut metadata = HashMap::new();
            metadata.insert("environment".into(), env_name.clone());
            metadata.insert("source".into(), "pm-envs|runtimes".into());
            TopologyNode {
                id: env_node_id.clone(),
                node_type: "environment".into(),
                status: "healthy".into(),
                metadata,
                deployed_classes: Vec::new(),
            }
        });

        // Build metadata for runtime
        let mut metadata = HashMap::new();
        metadata.insert("package".into(), rt.package_name.clone());
        metadata.insert("class".into(), rt.class_key.clone());
        metadata.insert("environment".into(), rt.target_environment.clone());
        metadata
            .insert("deployment_unit_id".into(), rt.deployment_unit_id.clone());
        if let Some(ref c) = rt.cluster_name {
            metadata.insert("cluster_name".into(), c.clone());
        }
        if let Some(ref st) = rt.status {
            metadata.insert("condition".into(), st.condition.clone());
            metadata.insert("phase".into(), st.phase.clone());
            if let Some(ref msg) = st.message {
                metadata.insert("message".into(), msg.clone());
            }
            metadata.insert("last_updated".into(), st.last_updated.clone());
        }

        runtime_nodes.push(TopologyNode {
            id: runtime_id.clone(),
            node_type: "runtime".into(),
            status: status.to_string(),
            metadata,
            deployed_classes: vec![rt.class_key.clone()],
        });

        // Connect runtime -> env
        edges.insert((runtime_id.clone(), entry.id.clone()));

        // Connect runtime -> deployment using (package,class,env) precise match, fallback to (package,class)
        if let Some(dep_node_id) = dep_id_by_pkg_class_env
            .get(&(
                rt.package_name.clone(),
                rt.class_key.clone(),
                rt.target_environment.clone(),
            ))
            .cloned()
            .or_else(|| {
                dep_id_by_pkg_class
                    .get(&(rt.package_name.clone(), rt.class_key.clone()))
                    .cloned()
            })
        {
            edges.insert((runtime_id.clone(), dep_node_id));
        }
    }

    for (env, node) in env_nodes.iter_mut() {
        if let Some(count) = env_counts.get(env) {
            node.metadata
                .insert("deployment_count".into(), count.to_string());
        }
        node.deployed_classes.sort();
        node.deployed_classes.dedup();
    }

    let mut nodes: Vec<TopologyNode> = env_nodes.into_values().collect();
    nodes.extend(deployment_nodes);
    nodes.extend(runtime_nodes);
    nodes.sort_by(|a, b| a.id.cmp(&b.id));

    let edges: Vec<(String, String)> = edges.into_iter().collect();

    Ok(TopologySnapshot {
        nodes,
        edges,
        timestamp: Utc::now().to_rfc3339(),
    })
}

#[cfg(feature = "server")]
fn derive_environments(
    deployment: &oprc_models::OClassDeployment,
) -> Vec<String> {
    use std::collections::BTreeSet;

    let mut envs: BTreeSet<String> = BTreeSet::new();

    if let Some(status) = &deployment.status {
        for env in &status.selected_envs {
            envs.insert(env.clone());
        }
    }

    if envs.is_empty() {
        for env in &deployment.target_envs {
            envs.insert(env.clone());
        }
    }

    // Note: do not include available_envs in topology mapping; availability alone
    // should not create edges or affect environment status. Keep them in metadata only.

    if envs.is_empty() {
        envs.insert("unassigned".into());
    }

    envs.into_iter().collect()
}

#[cfg(feature = "server")]
fn condition_to_status(condition: &DeploymentCondition) -> &'static str {
    match condition {
        DeploymentCondition::Running => "healthy",
        DeploymentCondition::Deploying | DeploymentCondition::Pending => {
            "degraded"
        }
        DeploymentCondition::Down | DeploymentCondition::Deleted => "down",
    }
}

#[cfg(feature = "server")]
fn status_rank(status: &str) -> u8 {
    match status {
        "healthy" => 0,
        "degraded" => 1,
        "down" => 2,
        _ => 1,
    }
}

#[cfg(feature = "server")]
async fn build_env_nodes_from_pm() -> Result<
    std::collections::BTreeMap<String, crate::types::TopologyNode>,
    ServerFnError,
> {
    use crate::types::TopologyNode;
    use reqwest::Client;
    use std::collections::{BTreeMap, HashMap};

    #[derive(serde::Deserialize)]
    struct PmClusterHealth {
        status: String,
        crm_version: Option<String>,
        last_seen: Option<String>,
        node_count: Option<u32>,
        ready_nodes: Option<u32>,
        availability: Option<f64>,
    }

    #[derive(serde::Deserialize)]
    struct PmClusterInfo {
        name: String,
        health: PmClusterHealth,
    }

    let base = crate::config::pm_base_url();
    let url = format!("{}/api/v1/envs", base);
    let client = Client::new();
    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(ServerFnError::new(format!(
            "PM /envs request failed: {}",
            resp.status()
        )));
    }

    let envs: Vec<PmClusterInfo> = resp
        .json()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    let mut map: BTreeMap<String, TopologyNode> = BTreeMap::new();
    for env in envs {
        let status = match env.health.status.as_str() {
            "Healthy" | "HEALTHY" | "healthy" => "healthy",
            "Degraded" | "DEGRADED" | "degraded" => "degraded",
            _ => "down",
        }
        .to_string();

        let mut metadata: HashMap<String, String> = HashMap::new();
        metadata.insert("environment".into(), env.name.clone());
        metadata.insert("source".into(), "pm-envs".into());
        if let Some(v) = env.health.crm_version {
            metadata.insert("crm_version".into(), v);
        }
        if let Some(v) = env.health.last_seen {
            metadata.insert("last_seen".into(), v);
        }
        if let Some(n) = env.health.node_count {
            metadata.insert("node_count".into(), n.to_string());
        }
        if let Some(n) = env.health.ready_nodes {
            metadata.insert("ready_nodes".into(), n.to_string());
        }
        if let Some(a) = env.health.availability {
            metadata.insert("availability".into(), format!("{a:.3}"));
        }

        map.insert(
            env.name.clone(),
            TopologyNode {
                id: format!("env::{}", env.name),
                node_type: "environment".into(),
                status,
                metadata,
                deployed_classes: Vec::new(),
            },
        );
    }

    Ok(map)
}

#[cfg(feature = "server")]
async fn fetch_class_runtimes() -> Result<Vec<ApiClassRuntime>, ServerFnError> {
    use crate::config::pm_base_url;
    use reqwest::Client;

    let url = format!("{}/api/v1/class-runtimes", pm_base_url());
    let client = Client::new();
    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(ServerFnError::new(format!(
            "PM /class-runtimes request failed: {}",
            resp.status()
        )));
    }

    resp.json::<Vec<ApiClassRuntime>>()
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
