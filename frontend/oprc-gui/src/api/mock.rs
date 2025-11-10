//! Centralized mock data builders for development mode.
//!
//! These helper functions isolate mock construction from real API proxy logic.
//! Extend this module with additional mock responses as needed.
#![cfg(feature = "server")]

use crate::types::{
    InvocationResponse, InvokeRequest, ObjData, ObjectGetRequest,
    PackageClassInfo, PackageFunctionInfo, PackageInfo, PackagesSnapshot,
    TopologyNode, TopologySnapshot,
};
use oprc_grpc::{ObjMeta, ValData};
use oprc_models::{
    DeploymentCondition, DeploymentStatusSummary, NfrRequirements,
    OClassDeployment,
};
use std::collections::HashMap;

/// Mock packages snapshot (static examples)
pub fn mock_packages_snapshot() -> PackagesSnapshot {
    let now = chrono::Utc::now().to_rfc3339();
    PackagesSnapshot {
        packages: vec![
            PackageInfo {
                name: "example.echo".into(),
                version: Some("0.1.0".into()),
                dependencies: vec![],
                classes: vec![PackageClassInfo {
                    key: "Echo".into(),
                    description: Some("Simple echo class".into()),
                    stateless_functions: vec!["echo".into()],
                    stateful_functions: vec![],
                }],
                functions: vec![PackageFunctionInfo {
                    key: "echo".into(),
                    function_type: "stateless".into(),
                    description: Some("Returns input".into()),
                }],
            },
            PackageInfo {
                name: "example.counter".into(),
                version: Some("0.2.0".into()),
                dependencies: vec!["example.echo".into()],
                classes: vec![PackageClassInfo {
                    key: "Counter".into(),
                    description: Some("Stateful counter".into()),
                    stateless_functions: vec!["reset".into()],
                    stateful_functions: vec!["inc".into(), "get".into()],
                }],
                functions: vec![
                    PackageFunctionInfo {
                        key: "reset".into(),
                        function_type: "stateless".into(),
                        description: Some("Reset counter".into()),
                    },
                    PackageFunctionInfo {
                        key: "inc".into(),
                        function_type: "stateful".into(),
                        description: Some("Increment".into()),
                    },
                    PackageFunctionInfo {
                        key: "get".into(),
                        function_type: "stateful".into(),
                        description: Some("Get value".into()),
                    },
                ],
            },
        ],
        timestamp: now,
    }
}

/// Mock topology snapshot
pub fn mock_topology_snapshot() -> TopologySnapshot {
    TopologySnapshot {
        nodes: vec![
            TopologyNode {
                id: "gateway-1".into(),
                node_type: "gateway".into(),
                status: "healthy".into(),
                metadata: HashMap::from([
                    ("env".into(), "cloud-1".into()),
                    ("port".into(), "8080".into()),
                ]),
                deployed_classes: vec![],
            },
            TopologyNode {
                id: "router-1".into(),
                node_type: "router".into(),
                status: "healthy".into(),
                metadata: HashMap::from([("env".into(), "cloud-1".into())]),
                deployed_classes: vec![],
            },
            TopologyNode {
                id: "odgm-1".into(),
                node_type: "odgm".into(),
                status: "healthy".into(),
                metadata: HashMap::from([
                    ("env".into(), "edge-1".into()),
                    ("collections".into(), "echo,counter".into()),
                ]),
                deployed_classes: vec![
                    "EchoClass".into(),
                    "CounterClass".into(),
                ],
            },
            TopologyNode {
                id: "odgm-2".into(),
                node_type: "odgm".into(),
                status: "healthy".into(),
                metadata: HashMap::from([
                    ("env".into(), "edge-2".into()),
                    ("collections".into(), "echo,counter".into()),
                ]),
                deployed_classes: vec![
                    "EchoClass".into(),
                    "CounterClass".into(),
                ],
            },
            TopologyNode {
                id: "fn-echo-1".into(),
                node_type: "function".into(),
                status: "healthy".into(),
                metadata: HashMap::from([
                    ("class".into(), "EchoClass".into()),
                    ("env".into(), "edge-1".into()),
                    ("replicas".into(), "2".into()),
                ]),
                deployed_classes: vec!["EchoClass".into()],
            },
        ],
        edges: vec![
            ("gateway-1".into(), "router-1".into()),
            ("router-1".into(), "odgm-1".into()),
            ("router-1".into(), "odgm-2".into()),
            ("router-1".into(), "fn-echo-1".into()),
        ],
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

/// Mock deployments list
pub fn mock_deployments() -> Vec<OClassDeployment> {
    let now = chrono::Utc::now();
    vec![
        OClassDeployment {
            key: "echo-fn-deployment".into(),
            package_name: "echo-service".into(),
            class_key: "EchoClass".into(),
            target_envs: vec!["edge-1".into(), "cloud-1".into()],
            available_envs: vec![
                "edge-1".into(),
                "cloud-1".into(),
                "edge-2".into(),
            ],
            nfr_requirements: NfrRequirements {
                min_throughput_rps: Some(100),
                availability: Some(0.99),
                cpu_utilization_target: Some(0.7),
            },
            condition: DeploymentCondition::Running,
            status: Some(DeploymentStatusSummary {
                replication_factor: 2,
                selected_envs: vec!["edge-1".into(), "cloud-1".into()],
                achieved_quorum_availability: Some(0.995),
                last_error: None,
            }),
            created_at: Some(now - chrono::Duration::hours(24)),
            updated_at: Some(now - chrono::Duration::minutes(30)),
            ..Default::default()
        },
        OClassDeployment {
            key: "counter-service".into(),
            package_name: "counter-pkg".into(),
            class_key: "Counter".into(),
            target_envs: vec!["cloud-1".into()],
            available_envs: vec!["cloud-1".into(), "cloud-2".into()],
            nfr_requirements: NfrRequirements {
                min_throughput_rps: Some(50),
                availability: Some(0.95),
                cpu_utilization_target: Some(0.8),
            },
            condition: DeploymentCondition::Deploying,
            status: Some(DeploymentStatusSummary {
                replication_factor: 1,
                selected_envs: vec!["cloud-1".into()],
                achieved_quorum_availability: None,
                last_error: None,
            }),
            created_at: Some(now - chrono::Duration::hours(2)),
            updated_at: Some(now - chrono::Duration::minutes(5)),
            ..Default::default()
        },
        OClassDeployment {
            key: "data-processor".into(),
            package_name: "processor-pkg".into(),
            class_key: "DataProcessor".into(),
            target_envs: vec!["edge-2".into()],
            available_envs: vec!["edge-1".into(), "edge-2".into()],
            nfr_requirements: NfrRequirements {
                min_throughput_rps: Some(200),
                availability: Some(0.98),
                cpu_utilization_target: Some(0.75),
            },
            condition: DeploymentCondition::Pending,
            status: Some(DeploymentStatusSummary {
                replication_factor: 1,
                selected_envs: vec![],
                achieved_quorum_availability: None,
                last_error: Some("Waiting for environment availability".into()),
            }),
            created_at: Some(now - chrono::Duration::minutes(10)),
            updated_at: Some(now - chrono::Duration::minutes(10)),
            ..Default::default()
        },
    ]
}

/// Mock object data for a given request
pub fn mock_object_data(req: &ObjectGetRequest) -> ObjData {
    let object_id = req.object_id.parse::<u64>().unwrap_or(0);
    let meta = ObjMeta {
        cls_id: req.class_key.clone(),
        partition_id: req.partition_id.parse().unwrap_or(0),
        object_id,
        object_id_str: if object_id == 0 {
            Some(req.object_id.clone())
        } else {
            None
        },
    };
    let value = ValData {
        data: serde_json::to_vec(&serde_json::json!({
            "counter": 42,
            "name": "Mock Object",
            "created_at": chrono::Utc::now().to_rfc3339(),
        }))
        .unwrap(),
        r#type: 0,
    };
    let mut entries = std::collections::HashMap::new();
    entries.insert(42u32, value);
    ObjData {
        metadata: Some(meta),
        entries,
        entries_str: std::collections::HashMap::new(),
        event: None,
    }
}

/// Mock invocation response
pub fn mock_invoke_response(req: &InvokeRequest) -> InvocationResponse {
    let payload = serde_json::to_vec(&serde_json::json!({
        "mock": true,
        "message": "Mock invoke result",
        "input": req.payload,
        "class_key": req.class_key,
        "function_key": req.function_key,
        "partition_id": req.partition_id,
        "object_id": req.object_id,
    }))
    .unwrap();

    InvocationResponse {
        payload: Some(payload),
        status: oprc_grpc::ResponseStatus::Okay as i32,
        headers: HashMap::from([(
            "content-type".to_string(),
            "application/json".to_string(),
        )]),
        invocation_id: "mock-invocation-id".to_string(),
    }
}
