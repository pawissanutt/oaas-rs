//! Topology server function implementations (deployments-based & zenoh-based)
//! Reconstructed after sidecar runtime refactor.

use crate::types::{TopologyRequest, TopologySnapshot};
use dioxus::prelude::*;
#[cfg(feature = "server")]
use oprc_models::DeploymentCondition;

// ────────────────────────────────────────────────────────────────────────────
// Server entry point
// ────────────────────────────────────────────────────────────────────────────
#[post("/api/proxy/topology")]
pub async fn proxy_topology(
    request: TopologyRequest,
) -> Result<TopologySnapshot, ServerFnError> {
    #[cfg(feature = "server")]
    {
        match request.source {
            TopologySource::Deployments => topology_from_deployments().await,
            TopologySource::Zenoh => topology_from_zenoh().await,
        }
    }
    #[cfg(not(feature = "server"))]
    {
        // Client/WASM build without server feature: return explicit error so UI can display message.
        Err(ServerFnError::new(
            "Topology server function unavailable (built without 'server' feature)",
        ))
    }
}

// ────────────────────────────────────────────────────────────────────────────
// Deployments-derived topology (Package Manager)
// ────────────────────────────────────────────────────────────────────────────
#[cfg(feature = "server")]
async fn topology_from_deployments() -> Result<TopologySnapshot, ServerFnError>
{
    use crate::types::TopologyNode;
    use chrono::Utc;
    use reqwest::Client;
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

    // Fetch environment nodes (with health metadata)
    let mut env_nodes = build_env_nodes_from_pm().await?; // BTreeMap<env, TopologyNode>

    // Fetch deployments from PM
    let deployments: Vec<oprc_models::OClassDeployment> =
        if crate::config::is_dev_mock() {
            crate::api::mock::mock_deployments()
        } else {
            let url =
                format!("{}/api/v1/deployments", crate::config::pm_base_url());
            let client = Client::new();
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;
            resp.json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?
        };

    let mut deployment_nodes: Vec<TopologyNode> = Vec::new();
    let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
    let mut env_counts: HashMap<String, usize> = HashMap::new();
    let mut dep_id_by_pkg_class_env: HashMap<(String, String, String), String> =
        HashMap::new();
    let mut dep_id_by_pkg_class: HashMap<(String, String), String> =
        HashMap::new();

    for dep in deployments.into_iter() {
        let envs = derive_environments(&dep);
        let status = condition_to_status(&dep.condition);
        for env in envs {
            let node_id = format!(
                "deployment::{}::{}::{}",
                dep.package_name, dep.class_key, env
            );
            let mut metadata = HashMap::new();
            metadata.insert("package".into(), dep.package_name.clone());
            metadata.insert("class".into(), dep.class_key.clone());
            metadata.insert("environment".into(), env.clone());
            metadata.insert("deployment_key".into(), dep.key.clone());
            if let Some(status_summary) = &dep.status {
                metadata.insert(
                    "selected_envs".into(),
                    status_summary.selected_envs.join(","),
                );
            }
            if let Some(odgm) = &dep.odgm {
                if !odgm.collections.is_empty() {
                    metadata.insert(
                        "odgm_collections".into(),
                        odgm.collections.join(","),
                    );
                }
            }
            deployment_nodes.push(TopologyNode {
                id: node_id.clone(),
                node_type: "deployment".into(),
                status: status.to_string(),
                metadata,
                deployed_classes: vec![dep.class_key.clone()],
            });
            env_counts
                .entry(env.clone())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            // Track mapping for runtime→deployment linking
            dep_id_by_pkg_class_env.insert(
                (dep.package_name.clone(), dep.class_key.clone(), env.clone()),
                node_id.clone(),
            );
            dep_id_by_pkg_class
                .entry((dep.package_name.clone(), dep.class_key.clone()))
                .or_insert(node_id.clone());
            // Edge deployment -> environment (direction chosen for GUI flow)
            let env_node_id = format!("env::{}", env);
            edges.insert((node_id.clone(), env_node_id));
            // Ensure env node records deployed class
            if let Some(en) = env_nodes.get_mut(&env) {
                en.deployed_classes.push(dep.class_key.clone());
            }
        }
    }

    // Fetch class runtimes for additional nodes/edges
    let runtimes = fetch_class_runtimes().await.unwrap_or_default();
    let mut runtime_nodes: Vec<TopologyNode> = Vec::new();
    for rt in runtimes.into_iter() {
        let runtime_id = format!("runtime::{}", rt.id);
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
        let mut metadata = HashMap::new();
        metadata.insert("package".into(), rt.package_name.clone());
        metadata.insert("class".into(), rt.class_key.clone());
        metadata.insert("environment".into(), rt.target_environment.clone());
        metadata
            .insert("deployment_unit_id".into(), rt.deployment_unit_id.clone());
        if let Some(c) = &rt.cluster_name {
            metadata.insert("cluster_name".into(), c.clone());
        }
        if let Some(st) = &rt.status {
            metadata.insert("condition".into(), st.condition.clone());
            metadata.insert("phase".into(), st.phase.clone());
            if let Some(msg) = &st.message {
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
        // Edge runtime -> environment
        edges.insert((
            runtime_id.clone(),
            format!("env::{}", rt.target_environment),
        ));
        // Edge runtime -> deployment (try env-specific first, fallback to pkg/class)
        if let Some(dep_id) = dep_id_by_pkg_class_env
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
            edges.insert((runtime_id.clone(), dep_id));
        }
    }

    // Finalize environment nodes (unique classes + deployment counts)
    for (env, node) in env_nodes.iter_mut() {
        if let Some(c) = env_counts.get(env) {
            node.metadata
                .insert("deployment_count".into(), c.to_string());
        }
        node.deployed_classes.sort();
        node.deployed_classes.dedup();
    }

    let mut nodes: Vec<TopologyNode> = env_nodes.into_values().collect();
    nodes.extend(deployment_nodes);
    nodes.extend(runtime_nodes);
    nodes.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(TopologySnapshot {
        nodes,
        edges: edges.into_iter().collect(),
        timestamp: Utc::now().to_rfc3339(),
    })
}

// ────────────────────────────────────────────────────────────────────────────
// Zenoh-derived topology (admin space) via sidecar runtime
// ────────────────────────────────────────────────────────────────────────────
#[cfg(feature = "server")]
async fn topology_from_zenoh() -> Result<TopologySnapshot, ServerFnError> {
    use crate::types::TopologyNode;
    use crate::zenoh_sidecar::ZENOH_SIDE_RT;
    use chrono::Utc;
    use oprc_zenoh::{Envconfig, OprcZenohConfig};
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    // Spawn the zenoh collection future on the dedicated sidecar runtime to avoid
    // calling block_on from within an existing Tokio runtime (which caused panic).
    let handle = ZENOH_SIDE_RT.spawn(async move {
        // Open session
        let cfg = match OprcZenohConfig::init_from_env() {
            Ok(c) => c,
            Err(e) => {
                return Err(ServerFnError::new(format!(
                    "zenoh config error: {e}"
                )));
            }
        };
        let session = match zenoh::open(cfg.create_zenoh()).await {
            Ok(s) => s,
            Err(e) => {
                return Err(ServerFnError::new(format!(
                    "zenoh open error: {e}"
                )));
            }
        };
        let key_expr: zenoh::key_expr::KeyExpr = "@/**".try_into().unwrap();
        let replies = match session.get(&key_expr).await {
            Ok(r) => r,
            Err(e) => {
                return Err(ServerFnError::new(format!(
                    "zenoh get admin error: {e}"
                )));
            }
        };
        let max_samples = 2000usize;
        let deadline = std::time::Duration::from_millis(1200);
        let mut count = 0usize;
        let mut routers: HashSet<String> = HashSet::new();
        let mut linkstate_edges: BTreeSet<(String, String)> = BTreeSet::new();
        let mut successor_edges: BTreeSet<(String, String)> = BTreeSet::new();
        struct Roles {
            classes: BTreeSet<String>,
            functions: BTreeSet<String>,
            odgm_objects: bool,
            other: BTreeSet<String>,
            sessions_client: usize,
            sessions_peer: usize,
            sessions_router: usize,
        }
        let mut roles_by_router: HashMap<String, Roles> = HashMap::new();
        let mut parse_dot_router_edges = |s: &str| -> Vec<(String, String)> {
            use regex::Regex;
            let mut idx_to_label: HashMap<String, String> = HashMap::new();
            let re_label =
                Regex::new(r#"\s*(\d+)\s*\[\s*label\s*=\s*"([^"]+)"\s*\]"#)
                    .ok();
            if let Some(re) = re_label.as_ref() {
                for line in s.lines() {
                    if let Some(caps) = re.captures(line) {
                        let idx = caps.get(1).unwrap().as_str().to_string();
                        let label = caps.get(2).unwrap().as_str().to_string();
                        let is_hex32 = label.len() == 32
                            && label.chars().all(|c| c.is_ascii_hexdigit());
                        if is_hex32 {
                            idx_to_label.insert(idx, label);
                        }
                    }
                }
            }
            let mut edges = Vec::new();
            let re_edge = Regex::new(r#"\s*(\d+)\s*--\s*(\d+)"#).ok();
            if let Some(re) = re_edge.as_ref() {
                for line in s.lines() {
                    if let Some(caps) = re.captures(line) {
                        let a = caps.get(1).unwrap().as_str();
                        let b = caps.get(2).unwrap().as_str();
                        if let (Some(la), Some(lb)) =
                            (idx_to_label.get(a), idx_to_label.get(b))
                        {
                            if la != lb {
                                edges.push((la.clone(), lb.clone()));
                            }
                        }
                    }
                }
            }
            edges
        };
        while count < max_samples {
            match tokio::time::timeout(deadline, replies.recv_async()).await {
                Ok(Ok(reply)) => match reply.result() {
                    Ok(sample) => {
                        let key = sample.key_expr().as_str();
                        count += 1;
                        if key.contains("/router/") {
                            if let Some(rid) =
                                extract_segment(key, "@/", "router")
                            {
                                routers.insert(rid);
                            }
                        }
                        if key.contains("/session/transport/unicast/") {
                            if let Some(rid) = extract_after(
                                key,
                                "/session/transport/unicast/",
                            ) {
                                let entry = roles_by_router
                                    .entry(rid)
                                    .or_insert(Roles {
                                        classes: BTreeSet::new(),
                                        functions: BTreeSet::new(),
                                        odgm_objects: false,
                                        other: BTreeSet::new(),
                                        sessions_client: 0,
                                        sessions_peer: 0,
                                        sessions_router: 0,
                                    });
                                let payload = sample.payload().to_bytes();
                                if let Ok(v) =
                                    serde_json::from_slice::<serde_json::Value>(
                                        &payload,
                                    )
                                {
                                    let kind = v
                                        .get("whatami")
                                        .and_then(|x| x.as_str())
                                        .unwrap_or("")
                                        .to_ascii_lowercase();
                                    match kind.as_str() {
                                        "client" => entry.sessions_client += 1,
                                        "peer" => entry.sessions_peer += 1,
                                        "router" => entry.sessions_router += 1,
                                        _ => {}
                                    }
                                }
                            }
                        }
                        if key.ends_with("/router/linkstate/routers") {
                            if let Ok(s) = std::str::from_utf8(
                                sample.payload().to_bytes().as_ref(),
                            ) {
                                for (a, b) in parse_dot_router_edges(s) {
                                    let (x, y) = order_pair(a, b);
                                    linkstate_edges.insert((x, y));
                                }
                            }
                        }
                        if key.contains("/router/route/successor/") {
                            if let (Some(src), Some(dst)) = (
                                extract_after(key, "/src/"),
                                extract_after(key, "/dst/"),
                            ) {
                                if src != dst {
                                    let (x, y) = order_pair(src, dst);
                                    successor_edges.insert((x, y));
                                }
                            }
                        }
                        if key.contains("/router/subscriber/")
                            || key.contains("/router/queryable/")
                        {
                            if let Some(rid) =
                                extract_segment(key, "@/", "router")
                            {
                                let topic = extract_tail_after(
                                    key,
                                    "/router/subscriber/",
                                )
                                .or_else(|| {
                                    extract_tail_after(
                                        key,
                                        "/router/queryable/",
                                    )
                                });
                                if let Some(topic) = topic {
                                    let parts: Vec<&str> =
                                        topic.split('/').collect();
                                    if parts.get(0) == Some(&"oprc") {
                                        let entry = roles_by_router
                                            .entry(rid)
                                            .or_insert(Roles {
                                                classes: BTreeSet::new(),
                                                functions: BTreeSet::new(),
                                                odgm_objects: false,
                                                other: BTreeSet::new(),
                                                sessions_client: 0,
                                                sessions_peer: 0,
                                                sessions_router: 0,
                                            });
                                        if let Some(cls) = parts.get(1) {
                                            entry
                                                .classes
                                                .insert((*cls).to_string());
                                        }
                                        if let Some(pos) = parts
                                            .iter()
                                            .position(|s| *s == "invokes")
                                        {
                                            if let Some(fn_name) =
                                                parts.get(pos + 1)
                                            {
                                                entry.functions.insert(
                                                    (*fn_name).to_string(),
                                                );
                                            }
                                        } else if parts
                                            .iter()
                                            .any(|s| *s == "objects")
                                        {
                                            entry.odgm_objects = true;
                                        } else {
                                            entry.other.insert(topic);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(_) => break,
                },
                Ok(Err(_)) => break,
                Err(_) => break,
            }
        }
        // Build nodes
        let mut nodes: Vec<TopologyNode> = Vec::new();
        for rid in routers.iter() {
            let roles = roles_by_router.get(rid);
            let mut metadata: HashMap<String, String> = HashMap::new();
            if let Some(r) = roles {
                if !r.functions.is_empty() {
                    metadata.insert(
                        "functions".into(),
                        r.functions
                            .iter()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(","),
                    );
                }
                if !r.classes.is_empty() {
                    metadata.insert(
                        "classes".into(),
                        r.classes.iter().cloned().collect::<Vec<_>>().join(","),
                    );
                }
                if r.odgm_objects {
                    metadata.insert("odgm_objects".into(), "true".into());
                }
                metadata.insert(
                    "sessions_client".into(),
                    r.sessions_client.to_string(),
                );
                metadata.insert(
                    "sessions_peer".into(),
                    r.sessions_peer.to_string(),
                );
                metadata.insert(
                    "sessions_router".into(),
                    r.sessions_router.to_string(),
                );
            }
            nodes.push(TopologyNode {
                id: format!("router::{rid}"),
                node_type: "router".into(),
                status: "healthy".into(),
                metadata,
                deployed_classes: roles
                    .map(|r| r.classes.iter().cloned().collect())
                    .unwrap_or_default(),
            });
        }
        // Function & class nodes
        let mut function_nodes: BTreeMap<String, BTreeSet<String>> =
            BTreeMap::new();
        let mut class_nodes: BTreeMap<String, BTreeSet<String>> =
            BTreeMap::new();
        for (rid, roles) in roles_by_router.iter() {
            for f in roles.functions.iter() {
                function_nodes
                    .entry(f.clone())
                    .or_default()
                    .insert(rid.clone());
            }
            for c in roles.classes.iter() {
                class_nodes
                    .entry(c.clone())
                    .or_default()
                    .insert(rid.clone());
            }
        }
        for (f, rset) in function_nodes.iter() {
            let mut md = HashMap::new();
            md.insert(
                "routers".into(),
                rset.iter().cloned().collect::<Vec<_>>().join(","),
            );
            nodes.push(TopologyNode {
                id: format!("function::{f}"),
                node_type: "function".into(),
                status: "healthy".into(),
                metadata: md,
                deployed_classes: Vec::new(),
            });
        }
        for (c, rset) in class_nodes.iter() {
            let mut md = HashMap::new();
            md.insert(
                "routers".into(),
                rset.iter().cloned().collect::<Vec<_>>().join(","),
            );
            nodes.push(TopologyNode {
                id: format!("class::{c}"),
                node_type: "class".into(),
                status: "healthy".into(),
                metadata: md,
                deployed_classes: vec![c.clone()],
            });
        }
        // Edges
        let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
        for (a, b) in linkstate_edges.iter() {
            edges.insert((format!("router::{a}"), format!("router::{b}")));
            edges.insert((format!("router::{b}"), format!("router::{a}")));
        }
        for (a, b) in successor_edges.iter() {
            edges.insert((format!("router::{a}"), format!("router::{b}")));
        }
        for (f, rset) in function_nodes.iter() {
            for r in rset.iter() {
                edges
                    .insert((format!("router::{r}"), format!("function::{f}")));
            }
        }
        for (c, rset) in class_nodes.iter() {
            for r in rset.iter() {
                edges.insert((format!("router::{r}"), format!("class::{c}")));
            }
        }
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok::<_, ServerFnError>(TopologySnapshot {
            nodes,
            edges: edges.into_iter().collect(),
            timestamp: Utc::now().to_rfc3339(),
        })
    });
    match handle.await {
        Ok(res) => res,
        Err(e) => {
            Err(ServerFnError::new(format!("zenoh sidecar join error: {e}")))
        }
    }
}

#[cfg(feature = "server")]
fn extract_segment(key: &str, root: &str, segment: &str) -> Option<String> {
    if !key.starts_with(root) {
        return None;
    }
    let path = &key[root.len()..];
    let parts: Vec<&str> = path.split('/').collect();
    for w in parts.windows(2) {
        if w[0] == segment {
            return Some(w[1].to_string());
        }
        if w[1] == segment {
            return Some(w[0].to_string());
        }
    }
    None
}

#[cfg(feature = "server")]
fn extract_after(key: &str, marker: &str) -> Option<String> {
    key.split(marker)
        .nth(1)
        .map(|s| s.split('/').next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(feature = "server")]
fn extract_tail_after(key: &str, marker: &str) -> Option<String> {
    if let Some(idx) = key.find(marker) {
        let start = idx + marker.len();
        if start <= key.len() {
            return Some(key[start..].to_string());
        }
    }
    None
}

#[cfg(feature = "server")]
fn order_pair(a: String, b: String) -> (String, String) {
    if a <= b { (a, b) } else { (b, a) }
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

// Local DTO mirror of PM ApiClassRuntime (subset used by topology). We duplicate
// minimal fields to avoid depending directly on PM crate server-only modules.
#[cfg(feature = "server")]
#[derive(Debug, Clone, serde::Deserialize)]
struct ApiClassRuntimeStatus {
    condition: String,
    phase: String,
    message: Option<String>,
    last_updated: String,
}

#[cfg(feature = "server")]
#[derive(Debug, Clone, serde::Deserialize)]
struct ApiClassRuntime {
    id: String,
    deployment_unit_id: String,
    package_name: String,
    class_key: String,
    target_environment: String,
    cluster_name: Option<String>,
    status: Option<ApiClassRuntimeStatus>,
}
