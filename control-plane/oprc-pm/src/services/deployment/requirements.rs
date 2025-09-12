use crate::services::deployment::cluster_quorum_availability; // from workflow.rs
use crate::services::deployment::required_replicas_quorum; // from workflow.rs
use crate::{
    crm::CrmManager,
    errors::{DeploymentError, PackageManagerError},
};
use oprc_models::OClass;
use oprc_models::OClassDeployment;
use tracing::info;

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct DeploymentRequirements {
    pub cpu_per_replica: f64,
    pub memory_per_replica: u64, // MB
    pub min_replicas: u32,
    pub max_replicas: u32,
    pub target_replicas: u32,
}

pub async fn calculate_requirements(
    crm_manager: &CrmManager,
    class: &OClass,
    deployment: &OClassDeployment,
) -> Result<DeploymentRequirements, PackageManagerError> {
    info!(
        "Calculating deployment requirements for class: {}",
        class.key
    );

    let mut target_availability = 0.99_f64; // default
    if let Some(dep_av) = deployment.nfr_requirements.availability {
        if (0.0..=1.0).contains(&dep_av) {
            target_availability = dep_av;
        }
    }

    let mut ratios: Vec<f64> = Vec::new();
    for cluster in &deployment.target_envs {
        let h = crm_manager.get_cluster_health(cluster).await.map_err(|e| {
            DeploymentError::Invalid(format!(
                "Failed to get cluster health for {cluster}: {e}"
            ))
        })?;
        if let Some(av) = h.availability {
            ratios.push(av.clamp(0.0, 1.0));
        } else if let (Some(r), Some(n)) = (h.ready_nodes, h.node_count) {
            if n == 0 {
                return Err(DeploymentError::Invalid(format!(
                    "Cluster {cluster} reported zero nodes"
                ))
                .into());
            }
            ratios.push((r as f64 / n as f64).clamp(0.0, 1.0));
        } else {
            return Err(DeploymentError::Invalid(format!(
                "Cluster {cluster} missing availability and ready/node counts"
            ))
            .into());
        }
    }
    if ratios.is_empty() {
        return Err(DeploymentError::Invalid(
            "No cluster health data available for availability calculation"
                .to_string(),
        )
        .into());
    }

    let mut sorted = ratios.clone();
    sorted
        .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut target_replicas =
        required_replicas_quorum(target_availability, &sorted, 50);
    let achieved =
        cluster_quorum_availability(&sorted[..target_replicas as usize]);

    if let Some(spec) = &class.state_spec {
        use oprc_models::ConsistencyModel;
        if spec.consistency_model == ConsistencyModel::Strong {
            let quorum = (target_replicas as usize / 2) + 1;
            let mut f = quorum.saturating_sub(1);
            if f == 0 {
                f = 1;
            }
            let strong_needed = 2 * f + 1;
            if strong_needed as u32 > target_replicas {
                target_replicas = strong_needed as u32;
            }
            if target_replicas > 50 {
                target_replicas = 50;
            }
        }
    }

    info!(class=%class.key, target_availability=%target_availability, selected_replicas=%target_replicas, achieved_quorum_availability=%achieved, ratios=?sorted, "Replica calculation (quorum+consistency model)");

    Ok(DeploymentRequirements {
        cpu_per_replica: 1.0,
        memory_per_replica: 512,
        min_replicas: 1,
        max_replicas: 100,
        target_replicas,
    })
}
