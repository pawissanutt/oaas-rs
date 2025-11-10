use super::{
    actions::FsmAction, descriptor::TemplateDescriptor, observed::Observed,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Pending,
    Applying,
    Progressing,
    Available,
    Degraded,
    Deleting,
}

pub struct EvalInput<'a> {
    pub phase: Option<Phase>,
    pub desc: &'a TemplateDescriptor,
    pub observed: &'a Observed,
    pub is_deleting: bool,
    pub generation_changed: bool,
    pub class_name: &'a str,
    pub functions_total: usize,
}

pub struct EvalOutput {
    pub phase: Phase,
    pub actions: Vec<FsmAction>,
}

pub fn evaluate(input: EvalInput) -> EvalOutput {
    if input.is_deleting {
        return EvalOutput {
            phase: Phase::Deleting,
            actions: vec![],
        };
    }

    // If generation changed we must (re)apply regardless of observed state.
    if input.generation_changed {
        return EvalOutput {
            phase: Phase::Applying,
            actions: vec![FsmAction::ApplyWorkload],
        };
    }

    // No children yet?
    if input.observed.children.is_empty() {
        // If we've already applied and generation hasn't changed, avoid re-applying repeatedly.
        if matches!(
            input.phase,
            Some(Phase::Applying) | Some(Phase::Progressing)
        ) && !input.generation_changed
        {
            return EvalOutput {
                phase: Phase::Applying,
                actions: vec![],
            };
        }
        // Otherwise trigger initial apply.
        return EvalOutput {
            phase: Phase::Applying,
            actions: vec![FsmAction::ApplyWorkload],
        };
    }

    // Evaluate descriptor readiness rules.
    let mut any_contributing = false;
    let mut all_ready = true;
    for rule in input.desc.readiness {
        if !rule.contributes_to_available {
            continue;
        }
        any_contributing = true;
        let result = (rule.evaluator)(input.observed);
        if !result.ready {
            all_ready = false;
        }
    }

    // Detect orphans: any managed function child not in expected name set.
    let total = input.functions_total;
    let mut expected: HashSet<String> = HashSet::new();
    for i in 0..total {
        expected.insert((input.desc.function_namer)(
            input.class_name,
            i,
            total,
        ));
    }
    // Determine which observed kinds correspond to primary function workloads for the
    // selected template descriptor. This prevents us from treating template-managed
    // auxiliary resources (e.g., Knative revisions, private services) as orphans.
    let mut managed_function_kinds: HashSet<&'static str> = HashSet::new();
    for kind in input.desc.managed {
        match kind {
            super::descriptor::ManagedKind::Deployment => {
                managed_function_kinds.insert("Deployment");
            }
            super::descriptor::ManagedKind::Service => {
                managed_function_kinds.insert("Service");
            }
            super::descriptor::ManagedKind::KnativeService => {
                managed_function_kinds.insert("KnativeService");
            }
            super::descriptor::ManagedKind::OdgmDeployment
            | super::descriptor::ManagedKind::OdgmService
            | super::descriptor::ManagedKind::Hpa => {}
        }
    }

    let is_function_kind = |k: &str| managed_function_kinds.contains(k);
    let mut orphans: Vec<String> = Vec::new();
    for c in &input.observed.children {
        // Skip known non-function children (ODGM resources) to avoid accidental deletion.
        // ODGM uses names `<class>-odgm` (Deployment) and `<class>-odgm-svc` (Service).
        if c.name.ends_with("-odgm") || c.name.ends_with("-odgm-svc") {
            continue;
        }
        if is_function_kind(&c.kind) && !expected.contains(&c.name) {
            orphans.push(c.name.clone());
        }
    }

    let mut actions: Vec<FsmAction> = Vec::new();
    if !orphans.is_empty() {
        actions.push(FsmAction::DeleteOrphans(orphans));
    }

    if any_contributing && all_ready {
        return EvalOutput {
            phase: Phase::Available,
            actions,
        };
    }

    // Potential future: distinguish degraded vs progressing based on timeouts / reasons.
    EvalOutput {
        phase: Phase::Progressing,
        actions,
    }
}
