use crate::types::{TopologyNode, TopologySnapshot};
use dioxus::prelude::*;

#[post("/api/proxy/topology")]
pub async fn proxy_topology() -> Result<TopologySnapshot, ServerFnError> {
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }

    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            use std::collections::HashMap;
            // Mock topology
            Ok(TopologySnapshot {
                nodes: vec![
                    TopologyNode {
                        id: "gateway-1".to_string(),
                        node_type: "gateway".to_string(),
                        status: "healthy".to_string(),
                        metadata: HashMap::from([
                            ("env".to_string(), "cloud-1".to_string()),
                            ("port".to_string(), "8080".to_string()),
                        ]),
                    },
                    TopologyNode {
                        id: "router-1".to_string(),
                        node_type: "router".to_string(),
                        status: "healthy".to_string(),
                        metadata: HashMap::from([(
                            "env".to_string(),
                            "cloud-1".to_string(),
                        )]),
                    },
                    TopologyNode {
                        id: "odgm-1".to_string(),
                        node_type: "odgm".to_string(),
                        status: "healthy".to_string(),
                        metadata: HashMap::from([
                            ("env".to_string(), "edge-1".to_string()),
                            (
                                "collections".to_string(),
                                "echo,counter".to_string(),
                            ),
                        ]),
                    },
                    TopologyNode {
                        id: "odgm-2".to_string(),
                        node_type: "odgm".to_string(),
                        status: "healthy".to_string(),
                        metadata: HashMap::from([
                            ("env".to_string(), "edge-2".to_string()),
                            (
                                "collections".to_string(),
                                "echo,counter".to_string(),
                            ),
                        ]),
                    },
                    TopologyNode {
                        id: "fn-echo-1".to_string(),
                        node_type: "function".to_string(),
                        status: "healthy".to_string(),
                        metadata: HashMap::from([
                            ("class".to_string(), "EchoClass".to_string()),
                            ("env".to_string(), "edge-1".to_string()),
                        ]),
                    },
                ],
                edges: vec![
                    ("gateway-1".to_string(), "router-1".to_string()),
                    ("router-1".to_string(), "odgm-1".to_string()),
                    ("router-1".to_string(), "odgm-2".to_string()),
                    ("router-1".to_string(), "fn-echo-1".to_string()),
                ],
                timestamp: chrono::Utc::now().to_rfc3339(),
            })
        } else {
            // Future: query Zenoh admin space
            Err(ServerFnError::new("Topology relay not yet implemented"))
        }
    }
}
