#[cfg(test)]
mod tests {
    use crate::controller::fsm::{
        ChildStatus, Observed, descriptors_builtin,
        evaluator::{EvalInput, Phase, evaluate},
    };

    fn observed_with(kind: &str, ready: bool) -> Observed {
        Observed {
            children: vec![ChildStatus {
                name: "x".into(),
                kind: kind.into(),
                ready,
                reason: None,
                message: None,
            }],
        }
    }

    #[test]
    fn k8s_descriptor_transitions_to_available_when_deployment_ready() {
        let desc = descriptors_builtin::descriptor_k8s();
        let observed = observed_with("Deployment", true);
        let out = evaluate(EvalInput {
            phase: Some(Phase::Progressing),
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed: false,
            class_name: "x",
            functions_total: 1,
        });
        assert_eq!(
            out.phase,
            Phase::Available,
            "Expected Available when deployment ready"
        );
    }

    #[test]
    fn k8s_descriptor_progressing_when_deployment_not_ready() {
        let desc = descriptors_builtin::descriptor_k8s();
        let observed = observed_with("Deployment", false);
        let out = evaluate(EvalInput {
            phase: Some(Phase::Applying),
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed: false,
            class_name: "x",
            functions_total: 1,
        });
        assert_eq!(out.phase, Phase::Progressing);
    }

    #[test]
    fn knative_descriptor_available_when_service_ready() {
        let desc = descriptors_builtin::descriptor_knative();
        let observed = observed_with("KnativeService", true);
        let out = evaluate(EvalInput {
            phase: Some(Phase::Progressing),
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed: false,
            class_name: "x",
            functions_total: 1,
        });
        assert_eq!(out.phase, Phase::Available);
    }

    #[test]
    fn knative_descriptor_progressing_when_service_not_ready() {
        let desc = descriptors_builtin::descriptor_knative();
        let observed = observed_with("KnativeService", false);
        let out = evaluate(EvalInput {
            phase: Some(Phase::Applying),
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed: false,
            class_name: "x",
            functions_total: 1,
        });
        assert_eq!(out.phase, Phase::Progressing);
    }
}
