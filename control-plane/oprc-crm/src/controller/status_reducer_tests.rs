#[cfg(test)]
mod tests {
    use super::super::status_reducer::merge_status;
    use crate::crd::class_runtime::{
        ClassRuntimeStatus, Condition, ConditionStatus, ConditionType,
        FunctionStatus,
    };

    fn base_status() -> ClassRuntimeStatus {
        ClassRuntimeStatus {
            phase: Some("progressing".into()),
            message: Some("initial".into()),
            observed_generation: Some(1),
            last_updated: None,
            conditions: Some(vec![Condition {
                type_: ConditionType::Progressing,
                status: ConditionStatus::True,
                reason: Some("ApplySuccessful".into()),
                message: Some("Resources applied".into()),
                last_transition_time: None,
            }]),
            resource_refs: None,
            nfr_recommendations: Some(
                [("replicas".to_string(), serde_json::json!(3))]
                    .into_iter()
                    .collect(),
            ),
            last_applied_recommendations: Some(vec![]),
            last_applied_at: None,
            routers: None,
            functions: Some(vec![FunctionStatus {
                function_key: "fn-a".into(),
                service: "svc-a".into(),
                port: 80,
                predicted_url: "http://svc-a".into(),
                observed_url: None,
                template: "dev".into(),
                ready: Some(true),
                reason: Some("Ready".into()),
                message: None,
                last_transition_time: None,
            }]),
        }
    }

    #[test]
    fn merge_preserves_recommendations_and_function_readiness() {
        let current = base_status();
        // Desired only sets phase/message, omits recommendations and function readiness
        let desired = ClassRuntimeStatus {
            phase: Some("progressing".into()),
            message: Some("tick".into()),
            observed_generation: Some(1),
            last_updated: None,
            conditions: Some(vec![Condition {
                type_: ConditionType::Progressing,
                status: ConditionStatus::True,
                reason: Some("ApplySuccessful".into()),
                message: Some("Resources applied".into()),
                last_transition_time: None,
            }]),
            resource_refs: None,
            nfr_recommendations: None,
            last_applied_recommendations: None,
            last_applied_at: None,
            routers: None,
            functions: None,
        };
        let merged = merge_status(Some(&current), desired);
        assert!(
            merged.nfr_recommendations.is_some(),
            "recommendations should be preserved"
        );
        assert!(merged.functions.as_ref().unwrap()[0].ready == Some(true));
        assert!(merged.last_applied_recommendations.is_some());
    }
}
