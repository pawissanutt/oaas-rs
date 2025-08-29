use crate::crd::class_runtime::{
    ClassRuntimeStatus, Condition, ConditionStatus, ConditionType,
};

pub fn progressing(
    now: String,
    generation: Option<i64>,
) -> ClassRuntimeStatus {
    ClassRuntimeStatus {
        // use canonical lowercase phase strings to match CRD/cluster validation
        phase: Some("progressing".to_string()),
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
