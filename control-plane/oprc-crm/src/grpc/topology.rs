use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};
use tracing::{debug, error};
use zenoh::Session;

use oprc_grpc::proto::topology::{
    TopologyEdge, TopologyNode, TopologyRequest, TopologySnapshot,
    topology_service_server::TopologyService,
};
use tracing::instrument;

pub struct TopologySvc {
    /// Cached client-mode session for admin queries (lazy initialized)
    admin_session: Mutex<Option<Arc<Session>>>,
}

impl TopologySvc {
    pub fn new(_zenoh: Arc<Session>) -> Self {
        Self {
            admin_session: Mutex::new(None),
        }
    }

    /// Get or create a client-mode session for admin queries.
    /// Client mode queries are forwarded to the router, giving us full admin visibility.
    async fn get_admin_session(&self) -> Result<Arc<Session>, anyhow::Error> {
        let mut guard = self.admin_session.lock().await;
        if let Some(session) = guard.as_ref() {
            return Ok(session.clone());
        }

        // Create a client-mode config connecting to the router
        // Use OPRC_ZENOH_PEERS env var (same as main session) to find router
        let zenoh_cfg = oprc_zenoh::OprcZenohConfig {
            mode: zenoh::config::WhatAmI::Client,
            peers: std::env::var("OPRC_ZENOH_PEERS").ok(),
            ..Default::default()
        };

        debug!(peers = ?zenoh_cfg.peers, "Creating client-mode Zenoh session for admin queries");
        let session =
            zenoh::open(zenoh_cfg.create_zenoh()).await.map_err(|e| {
                anyhow::anyhow!("Failed to open admin session: {}", e)
            })?;
        let session = Arc::new(session);
        *guard = Some(session.clone());
        Ok(session)
    }

    async fn fetch_zenoh_topology(
        &self,
    ) -> Result<TopologySnapshot, anyhow::Error> {
        // Use client-mode session for admin queries to get router's admin data
        let session = self.get_admin_session().await?;

        let key_expr: zenoh::key_expr::KeyExpr = "@/**".try_into().unwrap();
        let replies = session
            .get(&key_expr)
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

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

        let parse_dot_router_edges = |s: &str| -> Vec<(String, String)> {
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

        // Function nodes only (classes are logical, not Zenoh entities)
        // In Zenoh view, we show routers and the functions they serve
        let mut function_nodes: BTreeMap<String, BTreeSet<String>> =
            BTreeMap::new();
        let mut odgm_routers: BTreeSet<String> = BTreeSet::new();
        for (rid, roles) in roles_by_router.iter() {
            for f in roles.functions.iter() {
                function_nodes
                    .entry(f.clone())
                    .or_default()
                    .insert(rid.clone());
            }
            if roles.odgm_objects {
                odgm_routers.insert(rid.clone());
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

        // ODGM nodes - one per router that has odgm_objects
        for rid in odgm_routers.iter() {
            let mut md = HashMap::new();
            md.insert("router".into(), rid.clone());
            nodes.push(TopologyNode {
                id: format!("odgm::{rid}"),
                node_type: "odgm".into(),
                status: "healthy".into(),
                metadata: md,
                deployed_classes: Vec::new(),
            });
        }

        // Collect all valid node IDs for edge filtering
        let node_ids: HashSet<String> =
            nodes.iter().map(|n| n.id.clone()).collect();

        // Edges - only include edges where both endpoints exist as nodes
        let mut edges: Vec<TopologyEdge> = Vec::new();
        for (a, b) in linkstate_edges.iter() {
            let from_id = format!("router::{a}");
            let to_id = format!("router::{b}");
            if node_ids.contains(&from_id) && node_ids.contains(&to_id) {
                edges.push(TopologyEdge {
                    from_id: from_id.clone(),
                    to_id: to_id.clone(),
                    metadata: HashMap::new(),
                });
                edges.push(TopologyEdge {
                    from_id: to_id,
                    to_id: from_id,
                    metadata: HashMap::new(),
                });
            }
        }
        for (a, b) in successor_edges.iter() {
            let from_id = format!("router::{a}");
            let to_id = format!("router::{b}");
            if node_ids.contains(&from_id) && node_ids.contains(&to_id) {
                edges.push(TopologyEdge {
                    from_id,
                    to_id,
                    metadata: HashMap::new(),
                });
            }
        }
        for (f, rset) in function_nodes.iter() {
            let to_id = format!("function::{f}");
            for r in rset.iter() {
                let from_id = format!("router::{r}");
                if node_ids.contains(&from_id) && node_ids.contains(&to_id) {
                    edges.push(TopologyEdge {
                        from_id,
                        to_id: to_id.clone(),
                        metadata: HashMap::new(),
                    });
                }
            }
        }
        // Router -> ODGM edges
        for rid in odgm_routers.iter() {
            let from_id = format!("router::{rid}");
            let to_id = format!("odgm::{rid}");
            if node_ids.contains(&from_id) && node_ids.contains(&to_id) {
                edges.push(TopologyEdge {
                    from_id,
                    to_id,
                    metadata: HashMap::new(),
                });
            }
        }

        nodes.sort_by(|a, b| a.id.cmp(&b.id));

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
}

#[tonic::async_trait]
impl TopologyService for TopologySvc {
    #[instrument(level = "debug", skip(self, _request))]
    async fn get_topology(
        &self,
        _request: Request<TopologyRequest>,
    ) -> Result<Response<TopologySnapshot>, Status> {
        match self.fetch_zenoh_topology().await {
            Ok(snapshot) => Ok(Response::new(snapshot)),
            Err(e) => {
                error!("Failed to fetch zenoh topology: {}", e);
                Err(Status::internal(format!("Zenoh error: {}", e)))
            }
        }
    }
}

// Helper functions (copied from GUI implementation)

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

fn extract_after(key: &str, marker: &str) -> Option<String> {
    key.split(marker)
        .nth(1)
        .map(|s| s.split('/').next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
}

fn extract_tail_after(key: &str, marker: &str) -> Option<String> {
    if let Some(idx) = key.find(marker) {
        let start = idx + marker.len();
        if start <= key.len() {
            return Some(key[start..].to_string());
        }
    }
    None
}

fn order_pair(a: String, b: String) -> (String, String) {
    if a <= b { (a, b) } else { (b, a) }
}
