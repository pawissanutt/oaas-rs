use super::observed::Observed;
use crate::crd::class_runtime::FunctionStatus;

/// Update function statuses' readiness fields based on observed children.
/// Matches by service name present in FunctionStatus.service.
pub fn update_function_readiness(
    functions: &mut [FunctionStatus],
    observed: &Observed,
) {
    if functions.is_empty() {
        return;
    }
    for f in functions.iter_mut() {
        let svc = &f.service;
        let is_ready =
            observed.children.iter().any(|c| c.name == *svc && c.ready);
        if is_ready {
            f.ready = Some(true);
            f.reason = None;
            f.message = None;
        } else {
            f.ready = Some(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::fsm::observed::ChildStatus;
    use crate::crd::class_runtime::FunctionStatus;

    #[test]
    fn marks_function_ready_when_child_ready() {
        let mut funcs = vec![FunctionStatus {
            function_key: "k".into(),
            service: "svc-a".into(),
            port: 80,
            predicted_url: "".into(),
            observed_url: None,
            template: "k8s".into(),
            ready: None,
            reason: None,
            message: None,
            last_transition_time: None,
        }];
        let observed = Observed {
            children: vec![ChildStatus {
                name: "svc-a".into(),
                kind: "Deployment".into(),
                ready: true,
                reason: None,
                message: None,
            }],
        };
        update_function_readiness(&mut funcs, &observed);
        assert_eq!(funcs[0].ready, Some(true));
    }

    #[test]
    fn marks_function_not_ready_when_child_absent_or_unready() {
        let mut funcs = vec![FunctionStatus {
            function_key: "k".into(),
            service: "svc-a".into(),
            port: 80,
            predicted_url: "".into(),
            observed_url: None,
            template: "knative".into(),
            ready: None,
            reason: None,
            message: None,
            last_transition_time: None,
        }];
        let observed = Observed {
            children: vec![ChildStatus {
                name: "svc-a".into(),
                kind: "KnativeService".into(),
                ready: false,
                reason: None,
                message: None,
            }],
        };
        update_function_readiness(&mut funcs, &observed);
        assert_eq!(funcs[0].ready, Some(false));
    }
}
