#[cfg(test)]
mod tests {
    use crate::controller::fsm::{
        EvalInput, ManagedKind, Observed, Phase, ReadinessRule,
        TemplateDescriptor, evaluate,
    };

    fn dummy_desc() -> TemplateDescriptor {
        fn fn_namer(c: &str, i: usize, _t: usize) -> String {
            if _t == 1 {
                c.to_string()
            } else {
                format!("{}-fn-{}", c, i)
            }
        }
        fn lbl(c: &str) -> (String, String) {
            ("oaas.io/owner".into(), c.into())
        }
        static MANAGED: &[ManagedKind] =
            &[ManagedKind::Deployment, ManagedKind::Service];
        fn eval_ready(
            o: &Observed,
        ) -> crate::controller::fsm::descriptor::ReadinessResult {
            crate::controller::fsm::descriptor::ReadinessResult {
                ready: o.children.iter().all(|c| c.ready),
                reason: None,
                message: None,
            }
        }
        static RULES: &[ReadinessRule] = &[ReadinessRule {
            kind: ManagedKind::Deployment,
            contributes_to_available: true,
            evaluator: eval_ready,
        }];
        TemplateDescriptor {
            id: "dev",
            managed: MANAGED,
            readiness: RULES,
            function_namer: fn_namer,
            label_selector: lbl,
            supports_odgm: true,
        }
    }

    #[test]
    fn evaluate_applies_when_no_children_or_generation_changed() {
        let desc = dummy_desc();
        let observed = Observed { children: vec![] };
        let out = evaluate(EvalInput {
            phase: None,
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed: true,
            class_name: "myclass",
            functions_total: 0,
        });
        assert_eq!(out.phase, Phase::Applying);
    }

    #[test]
    fn evaluate_available_when_all_children_ready() {
        let desc = dummy_desc();
        let observed = Observed {
            children: vec![crate::controller::fsm::observed::ChildStatus {
                name: "myclass".into(),
                kind: "Deployment".into(),
                ready: true,
                reason: None,
                message: None,
            }],
        };
        let out = evaluate(EvalInput {
            phase: Some(Phase::Progressing),
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed: false,
            class_name: "myclass",
            functions_total: 1,
        });
        assert_eq!(out.phase, Phase::Available);
    }

    #[test]
    fn evaluate_emits_delete_orphans_when_extra_function_children_exist() {
        let desc = dummy_desc();
        let observed = Observed {
            children: vec![
                crate::controller::fsm::observed::ChildStatus {
                    name: "myclass".into(),
                    kind: "Deployment".into(),
                    ready: true,
                    reason: None,
                    message: None,
                },
                crate::controller::fsm::observed::ChildStatus {
                    name: "myclass-fn-2".into(),
                    kind: "Deployment".into(),
                    ready: true,
                    reason: None,
                    message: None,
                },
            ],
        };
        let out = evaluate(EvalInput {
            phase: Some(Phase::Progressing),
            desc: &desc,
            observed: &observed,
            is_deleting: false,
            generation_changed: false,
            class_name: "myclass",
            functions_total: 1, // expect only one function named "myclass"
        });
        let has_delete = out.actions.iter().any(|a| matches!(a, crate::controller::fsm::actions::FsmAction::DeleteOrphans(list) if list.contains(&"myclass-fn-2".into())));
        assert!(
            has_delete,
            "expected DeleteOrphans for unexpected child name"
        );
    }
}
