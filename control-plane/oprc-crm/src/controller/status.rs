use crate::crd::deployment_record::{
    Condition, ConditionStatus, ConditionType, DeploymentRecordStatus,
};

pub fn progressing(
    now: String,
    generation: Option<i64>,
) -> DeploymentRecordStatus {
    DeploymentRecordStatus {
        phase: Some("Progressing".to_string()),
        message: Some("Applied workload".to_string()),
        observed_generation: generation,
        last_updated: Some(now.clone()),
        conditions: Some(vec![Condition {
            type_: ConditionType::Progressing,
            status: ConditionStatus::True,
            reason: Some("ApplySuccessful".into()),
            message: Some("Resources applied".into()),
            last_transition_time: Some(now),
        }]),
        resource_refs: None,
        nfr_recommendations: None,
        last_applied_recommendations: None,
        last_applied_at: None,
    }
}
