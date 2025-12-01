use crate::types::ConnectionArgs;
use anyhow::Result;
use serde_json::Value;
use std::convert::TryInto;
use tokio::time::{Duration, timeout};
use zenoh::key_expr::KeyExpr;

pub async fn handle_zenoh_admin_command(
    conn: &ConnectionArgs,
    key: &str,
    json: bool,
    limit: usize,
    timeout_secs: u64,
) -> Result<()> {
    let session = conn.open_zenoh().await;

    // Parse key expression (allow raw admin keys like "@/**")
    let ke: KeyExpr = key
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid key expression: {}", key))?;

    let replies = session.get(&ke).await.map_err(|_| {
        anyhow::anyhow!("failed to issue zenoh get for {}", key)
    })?;

    let mut out: Vec<Value> = Vec::new();
    let mut count = 0usize;
    let deadline = Duration::from_secs(timeout_secs);

    loop {
        if limit > 0 && count >= limit {
            break;
        }
        match timeout(deadline, replies.recv_async()).await {
            Ok(Ok(reply)) => match reply.result() {
                Ok(sample) => {
                    let key_str = sample.key_expr().as_str().to_string();
                    let payload = sample.payload().to_bytes();
                    let val = match serde_json::from_slice::<Value>(&payload) {
                        Ok(v) => v,
                        Err(_) => {
                            // Fallback: try UTF-8 string else base64
                            match std::str::from_utf8(&payload) {
                                Ok(s) => Value::String(s.to_string()),
                                Err(_) => {
                                    use base64::prelude::*;
                                    Value::String(
                                        BASE64_STANDARD.encode(&payload),
                                    )
                                }
                            }
                        }
                    };
                    out.push(json_object(key_str, val));
                    count += 1;
                }
                Err(_) => {
                    // Ignore errors to allow collecting other replies
                }
            },
            Ok(Err(_)) => break, // stream closed
            Err(_) => break,     // timeout waiting for more
        }
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        for v in out.iter() {
            let key = v.get("key").and_then(|x| x.as_str()).unwrap_or("");
            println!("key: {}", key);
            if let Some(val) = v.get("value") {
                println!("value: {}", serde_json::to_string_pretty(val)?);
            }
            println!("---");
        }
    }

    Ok(())
}

fn json_object(key: String, value: Value) -> Value {
    serde_json::json!({ "key": key, "value": value })
}

pub async fn handle_zenoh_admin_topology(
    conn: &ConnectionArgs,
    key: &str,
    json: bool,
    ascii: bool,
    limit: usize,
    timeout_secs: u64,
) -> Result<()> {
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
    let session = conn.open_zenoh().await;

    // Collect admin data from a few known patterns if the provided pattern is too broad
    // Default strategy: query provided key (which defaults to @/**) and extract routers/sessions
    let ke: KeyExpr = key
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid key expression: {}", key))?;
    let replies = session.get(&ke).await.map_err(|_| {
        anyhow::anyhow!("failed to issue zenoh get for {}", key)
    })?;

    let deadline = Duration::from_secs(timeout_secs);
    let mut count = 0usize;
    let mut router_ids: HashSet<String> = HashSet::new();
    // Distinguish sessions by whatami; store session zids per type
    let mut session_zids: HashMap<String, BTreeSet<String>> = HashMap::new();
    // Count sessions per owning router (from key path)
    let mut sessions_per_owner: BTreeMap<String, usize> = BTreeMap::new();
    // Keep edges separated by origin
    let mut linkstate_edges: BTreeSet<(String, String)> = BTreeSet::new(); // undirected unique
    let mut successor_edges: BTreeSet<(String, String)> = BTreeSet::new(); // undirected unique
    // Roles derived from subscription/queryable topics per router
    #[derive(Default)]
    struct RouterRoles {
        functions: BTreeSet<String>,
        odgm_objects: bool,
        classes: BTreeSet<String>,
        other: BTreeSet<String>,
    }
    let mut router_roles: BTreeMap<String, RouterRoles> = BTreeMap::new();

    loop {
        if limit > 0 && count >= limit {
            break;
        }
        match timeout(deadline, replies.recv_async()).await {
            Ok(Ok(reply)) => match reply.result() {
                Ok(sample) => {
                    let key_str = sample.key_expr().as_str();
                    // Extract basic IDs from key path
                    if let Some(id) = extract_segment(key_str, "@/", "router") {
                        router_ids.insert(id);
                    }
                    // Session classification from transport keys with JSON payload (whatami, zid)
                    if key_str.contains("/session/transport/unicast/") {
                        if let Some(owner) = extract_after(
                            key_str,
                            "/session/transport/unicast/",
                        ) {
                            *sessions_per_owner
                                .entry(owner.clone())
                                .or_insert(0) += 1;
                        }
                        let payload = sample.payload().to_bytes();
                        if let Ok(v) = serde_json::from_slice::<Value>(&payload)
                        {
                            let whatami = v
                                .get("whatami")
                                .and_then(|x| x.as_str())
                                .unwrap_or("unknown")
                                .to_ascii_lowercase();
                            let zid = v
                                .get("zid")
                                .and_then(|x| x.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| "unknown".to_string());
                            session_zids
                                .entry(whatami)
                                .or_default()
                                .insert(zid);
                        }
                    }
                    // Try to infer router-router edges from linkstate/route or peers
                    // 1) linkstate graph edges are inside payload as DOT; parse minimal: find 'label = "<id>"' pairs
                    if key_str.ends_with("/router/linkstate/routers") {
                        if let Ok(s) = std::str::from_utf8(
                            sample.payload().to_bytes().as_ref(),
                        ) {
                            for (a, b) in parse_dot_router_edges(s) {
                                if a != b {
                                    let (x, y) = order_pair(a, b);
                                    router_ids.insert(x.clone());
                                    router_ids.insert(y.clone());
                                    linkstate_edges.insert((x, y));
                                }
                            }
                        }
                    }
                    // 2) route successor keys contain src/dst IDs in the path
                    if key_str.contains("/router/route/successor/") {
                        if let (Some(src), Some(dst)) = (
                            extract_after(key_str, "/src/"),
                            extract_after(key_str, "/dst/"),
                        ) {
                            if src != dst {
                                let (x, y) =
                                    order_pair(src.clone(), dst.clone());
                                successor_edges.insert((x, y));
                                router_ids.insert(src);
                                router_ids.insert(dst);
                            }
                        }
                    }
                    // 3) subscriptions/queryables under opRC namespace -> infer functions and ODGM
                    if key_str.contains("/router/subscriber/")
                        || key_str.contains("/router/queryable/")
                    {
                        if let Some(rid) =
                            extract_segment(key_str, "@/", "router")
                        {
                            let topic = if let Some(t) = extract_tail_after(
                                key_str,
                                "/router/subscriber/",
                            ) {
                                t
                            } else if let Some(t) = extract_tail_after(
                                key_str,
                                "/router/queryable/",
                            ) {
                                t
                            } else {
                                String::new()
                            };
                            if !topic.is_empty() {
                                let parts: Vec<&str> =
                                    topic.split('/').collect();
                                if parts.get(0) == Some(&"oprc") {
                                    let entry =
                                        router_roles.entry(rid).or_default();
                                    let class_opt =
                                        parts.get(1).map(|s| s.to_string());
                                    // Look for common channels
                                    if let Some(pos) = parts
                                        .iter()
                                        .position(|s| *s == "invokes")
                                    {
                                        if let Some(fn_name) =
                                            parts.get(pos + 1)
                                        {
                                            entry
                                                .functions
                                                .insert((*fn_name).to_string());
                                        } else {
                                            entry.other.insert(topic);
                                        }
                                        if let Some(cls) = class_opt.as_ref() {
                                            entry.classes.insert(cls.clone());
                                        }
                                    } else if let Some(_pos) = parts
                                        .iter()
                                        .position(|s| *s == "objects")
                                    {
                                        // ODGM object space
                                        entry.odgm_objects = true;
                                        if let Some(cls) = class_opt.as_ref() {
                                            entry.classes.insert(cls.clone());
                                        }
                                    } else {
                                        entry.other.insert(topic);
                                    }
                                }
                            }
                        }
                    }
                    count += 1;
                }
                Err(_) => {}
            },
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    if ascii && !json {
        // Build ASCII representation
        // Aggregate classes & functions across all routers
        let mut all_classes: BTreeSet<String> = BTreeSet::new();
        let mut odgm_routers: Vec<String> = Vec::new();
        for (rid, roles) in router_roles.iter() {
            if roles.odgm_objects {
                odgm_routers.push(rid.clone());
            }
            for c in roles.classes.iter() {
                all_classes.insert(c.clone());
            }
        }

        // Header
        println!("+---------------- ODGM ----------------+");
        if !all_classes.is_empty() {
            let cls_line =
                all_classes.iter().cloned().collect::<Vec<_>>().join(", ");
            println!("| Classes: {}", cls_line);
        } else {
            println!("| (no classes observed)");
        }
        if !odgm_routers.is_empty() {
            println!("| Routers attached: {}", odgm_routers.join(", "));
        }
        println!("+--------------------------------------+\n");

        // Router blocks
        println!("Routers ({}):", router_ids.len());
        for rid in router_ids.iter() {
            let roles = router_roles.get(rid);
            println!("[{}]", rid);
            if let Some(r) = roles {
                if r.odgm_objects {
                    println!("  -> ODGM");
                }
                if !r.classes.is_empty() {
                    println!("  Classes:");
                    for c in r.classes.iter() {
                        println!("    - {}", c);
                    }
                }
                if !r.functions.is_empty() {
                    println!("  Functions:");
                    for f in r.functions.iter() {
                        println!("    - fn:{}", f);
                    }
                }
                if !r.other.is_empty() {
                    println!("  Other topics:");
                    for o in r.other.iter() {
                        println!("    - {}", o);
                    }
                }
            } else {
                println!("  (no roles detected)");
            }
            println!();
        }

        // ASCII edges summary
        println!("Edges:");
        if linkstate_edges.is_empty() && successor_edges.is_empty() {
            println!("  (none)");
        } else {
            for (a, b) in linkstate_edges.iter() {
                println!("  {} <-> {} (linkstate)", a, b);
            }
            for (a, b) in successor_edges.iter() {
                println!("  {} -> {} (successor)", a, b);
            }
        }

        // Sessions summary
        let total_sessions: usize =
            session_zids.values().map(|s| s.len()).sum();
        println!("Sessions total: {}", total_sessions);
        if total_sessions > 0 {
            let clients =
                session_zids.get("client").map(|s| s.len()).unwrap_or(0);
            let peers = session_zids.get("peer").map(|s| s.len()).unwrap_or(0);
            let routers_s =
                session_zids.get("router").map(|s| s.len()).unwrap_or(0);
            println!(
                "  types: client={} peer={} router={} other={}",
                clients,
                peers,
                routers_s,
                total_sessions.saturating_sub(clients + peers + routers_s)
            );
        }
        return Ok(());
    }
    if json {
        #[derive(serde::Serialize)]
        struct Node {
            id: String,
            node_type: String,
        }
        #[derive(serde::Serialize)]
        struct Edge {
            source: String,
            target: String,
            kind: &'static str,
        }
        #[derive(serde::Serialize)]
        struct RouterInfo {
            id: String,
            functions: Vec<String>,
            odgm_objects: bool,
            classes: Vec<String>,
        }
        #[derive(serde::Serialize)]
        struct Topo {
            routers: Vec<Node>,
            sessions: Vec<Node>,
            edges: Vec<Edge>,
            sessions_per_owner: BTreeMap<String, usize>,
            router_info: Vec<RouterInfo>,
        }

        let routers: Vec<Node> = router_ids
            .iter()
            .cloned()
            .map(|id| Node {
                id,
                node_type: "router".to_string(),
            })
            .collect();

        let mut sessions: Vec<Node> = Vec::new();
        for (ty, zids) in session_zids.iter() {
            for zid in zids.iter() {
                sessions.push(Node {
                    id: zid.clone(),
                    node_type: ty.clone(),
                });
            }
        }

        let mut edges: Vec<Edge> = Vec::new();
        for (a, b) in linkstate_edges.into_iter() {
            edges.push(Edge {
                source: a,
                target: b,
                kind: "linkstate",
            });
        }
        for (a, b) in successor_edges.into_iter() {
            edges.push(Edge {
                source: a,
                target: b,
                kind: "successor",
            });
        }

        let mut router_info: Vec<RouterInfo> = Vec::new();
        for (rid, roles) in router_roles.iter() {
            router_info.push(RouterInfo {
                id: rid.clone(),
                functions: roles.functions.iter().cloned().collect(),
                odgm_objects: roles.odgm_objects,
                classes: roles.classes.iter().cloned().collect(),
            });
        }

        println!(
            "{}",
            serde_json::to_string_pretty(&Topo {
                routers,
                sessions,
                edges,
                sessions_per_owner,
                router_info
            })?
        );
    } else {
        println!("Routers: {}", router_ids.len());
        for id in router_ids.iter() {
            println!("  - {}", id);
        }
        let total_ls = linkstate_edges.len();
        let total_succ = successor_edges.len();
        println!("Router links (linkstate): {}", total_ls);
        for (a, b) in linkstate_edges.iter() {
            println!("  {} <-> {}", a, b);
        }
        println!("Router links (successor): {}", total_succ);
        for (a, b) in successor_edges.iter() {
            println!("  {} -> {}", a, b);
        }
        let total_sessions: usize =
            session_zids.values().map(|s| s.len()).sum();
        println!("Sessions: {}", total_sessions);
        if total_sessions > 0 {
            let clients =
                session_zids.get("client").map(|s| s.len()).unwrap_or(0);
            let peers = session_zids.get("peer").map(|s| s.len()).unwrap_or(0);
            let routers =
                session_zids.get("router").map(|s| s.len()).unwrap_or(0);
            let unknown =
                total_sessions.saturating_sub(clients + peers + routers);
            println!(
                "  by type: clients={}, peers={}, routers={}, unknown={}",
                clients, peers, routers, unknown
            );
            if !sessions_per_owner.is_empty() {
                println!("  by owner router:");
                for (rid, n) in sessions_per_owner.iter() {
                    println!("    {}: {}", rid, n);
                }
            }
        }
        if !router_roles.is_empty() {
            println!("Roles by router:");
            for (rid, roles) in router_roles.iter() {
                let mut funcs: Vec<String> =
                    roles.functions.iter().cloned().collect();
                funcs.sort();
                println!("  - {}:", rid);
                if !funcs.is_empty() {
                    println!("      functions: {}", funcs.join(", "));
                }
                if roles.odgm_objects {
                    println!("      odgm: objects=true");
                }
                if !roles.classes.is_empty() {
                    let mut classes: Vec<String> =
                        roles.classes.iter().cloned().collect();
                    classes.sort();
                    println!("      classes: {}", classes.join(", "));
                }
                if !roles.other.is_empty() {
                    let mut other: Vec<String> =
                        roles.other.iter().cloned().collect();
                    other.sort();
                    println!("      other: {}", other.join(", "));
                }
            }
        }
    }

    Ok(())
}

fn extract_segment<'a>(
    key: &'a str,
    root: &str,
    segment: &str,
) -> Option<String> {
    // Expect format like "@/<id>/<segment>/..." or "@/<segment>/<id>/..."
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

fn extract_after<'a>(key: &'a str, marker: &str) -> Option<String> {
    key.split(marker)
        .nth(1)
        .map(|s| s.split('/').next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
}

fn order_pair(a: String, b: String) -> (String, String) {
    if a <= b { (a, b) } else { (b, a) }
}

fn parse_dot_router_edges(s: &str) -> Vec<(String, String)> {
    // Minimal parse: find lines like '1 -- 0 [ label = "..." ]' and map to labels in earlier lines
    // We'll collect index->label first
    use regex::Regex;
    let mut idx_to_label: std::collections::HashMap<String, String> =
        Default::default();
    let re_label =
        Regex::new(r#"\s*(\d+)\s*\[\s*label\s*=\s*"([^"]+)"\s*\]"#).ok();
    if let Some(re) = re_label.as_ref() {
        for line in s.lines() {
            if let Some(caps) = re.captures(line) {
                let idx = caps.get(1).unwrap().as_str().to_string();
                let label = caps.get(2).unwrap().as_str().to_string();
                // Only accept labels that look like 32-hex ZIDs to avoid weights or misc labels
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
                    edges.push((la.clone(), lb.clone()));
                }
            }
        }
    }
    edges
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
