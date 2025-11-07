//! Deployments management proxy

use dioxus::prelude::*;
use oprc_models::OClassDeployment;

#[post("/api/proxy/deployments")]
pub async fn proxy_deployments() -> Result<Vec<OClassDeployment>, ServerFnError>
{
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }

    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            // Mock deployment data
            use oprc_models::{
                DeploymentCondition, DeploymentStatusSummary, NfrRequirements,
            };
            let now = chrono::Utc::now();
            Ok(vec![
                OClassDeployment {
                    key: "echo-fn-deployment".to_string(),
                    package_name: "echo-service".to_string(),
                    class_key: "EchoClass".to_string(),
                    target_envs: vec![
                        "edge-1".to_string(),
                        "cloud-1".to_string(),
                    ],
                    available_envs: vec![
                        "edge-1".to_string(),
                        "cloud-1".to_string(),
                        "edge-2".to_string(),
                    ],
                    nfr_requirements: NfrRequirements {
                        min_throughput_rps: Some(100),
                        availability: Some(0.99),
                        cpu_utilization_target: Some(0.7),
                    },
                    condition: DeploymentCondition::Running,
                    status: Some(DeploymentStatusSummary {
                        replication_factor: 2,
                        selected_envs: vec![
                            "edge-1".to_string(),
                            "cloud-1".to_string(),
                        ],
                        achieved_quorum_availability: Some(0.995),
                        last_error: None,
                    }),
                    created_at: Some(now - chrono::Duration::hours(24)),
                    updated_at: Some(now - chrono::Duration::minutes(30)),
                    ..Default::default()
                },
                OClassDeployment {
                    key: "counter-service".to_string(),
                    package_name: "counter-pkg".to_string(),
                    class_key: "Counter".to_string(),
                    target_envs: vec!["cloud-1".to_string()],
                    available_envs: vec![
                        "cloud-1".to_string(),
                        "cloud-2".to_string(),
                    ],
                    nfr_requirements: NfrRequirements {
                        min_throughput_rps: Some(50),
                        availability: Some(0.95),
                        cpu_utilization_target: Some(0.8),
                    },
                    condition: DeploymentCondition::Deploying,
                    status: Some(DeploymentStatusSummary {
                        replication_factor: 1,
                        selected_envs: vec!["cloud-1".to_string()],
                        achieved_quorum_availability: None,
                        last_error: None,
                    }),
                    created_at: Some(now - chrono::Duration::hours(2)),
                    updated_at: Some(now - chrono::Duration::minutes(5)),
                    ..Default::default()
                },
                OClassDeployment {
                    key: "data-processor".to_string(),
                    package_name: "processor-pkg".to_string(),
                    class_key: "DataProcessor".to_string(),
                    target_envs: vec!["edge-2".to_string()],
                    available_envs: vec![
                        "edge-1".to_string(),
                        "edge-2".to_string(),
                    ],
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
                        last_error: Some(
                            "Waiting for environment availability".to_string(),
                        ),
                    }),
                    created_at: Some(now - chrono::Duration::minutes(10)),
                    updated_at: Some(now - chrono::Duration::minutes(10)),
                    ..Default::default()
                },
            ])
        } else {
            // Relay to PM: GET /api/v1/deployments
            let url =
                format!("{}/api/v1/deployments", crate::config::pm_base_url());
            let client = reqwest::Client::new();
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))?;

            resp.json()
                .await
                .map_err(|e| ServerFnError::new(e.to_string()))
        }
    }
}
