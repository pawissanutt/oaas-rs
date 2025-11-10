use super::descriptor::{
    ManagedKind, ReadinessResult, ReadinessRule, TemplateDescriptor,
};
use super::observed::Observed;

fn label_selector(class: &str) -> (String, String) {
    ("oaas.io/owner".into(), class.into())
}
fn function_namer(class: &str, idx: usize, total: usize) -> String {
    crate::routing::function_service_name(class, idx, total)
}

// Readiness evaluators
fn deployments_ready(obs: &Observed) -> ReadinessResult {
    let ds: Vec<_> = obs
        .children
        .iter()
        .filter(|c| c.kind == "Deployment")
        .collect();
    if ds.is_empty() {
        return ReadinessResult {
            ready: false,
            reason: Some("NoDeployments".into()),
            message: None,
        };
    }
    let all_ready = ds.iter().all(|c| c.ready);
    ReadinessResult {
        ready: all_ready,
        reason: None,
        message: None,
    }
}

fn knative_services_ready(obs: &Observed) -> ReadinessResult {
    let ks: Vec<_> = obs
        .children
        .iter()
        .filter(|c| c.kind == "KnativeService")
        .collect();
    if ks.is_empty() {
        return ReadinessResult {
            ready: false,
            reason: Some("NoKnativeServices".into()),
            message: None,
        };
    }
    let all_ready = ks.iter().all(|c| c.ready);
    ReadinessResult {
        ready: all_ready,
        reason: None,
        message: None,
    }
}

// Built-in descriptors
static K8S_MANAGED: &[ManagedKind] = &[
    ManagedKind::Deployment,
    ManagedKind::Service,
    ManagedKind::OdgmDeployment,
    ManagedKind::OdgmService,
];

static K8S_RULES: &[ReadinessRule] = &[ReadinessRule {
    kind: ManagedKind::Deployment,
    contributes_to_available: true,
    evaluator: deployments_ready,
}];

static KNATIVE_MANAGED: &[ManagedKind] = &[
    ManagedKind::KnativeService,
    ManagedKind::OdgmDeployment,
    ManagedKind::OdgmService,
];

static KNATIVE_RULES: &[ReadinessRule] = &[ReadinessRule {
    kind: ManagedKind::KnativeService,
    contributes_to_available: true,
    evaluator: knative_services_ready,
}];

pub fn descriptor_k8s() -> TemplateDescriptor {
    TemplateDescriptor {
        id: "k8s_deployment",
        managed: K8S_MANAGED,
        readiness: K8S_RULES,
        function_namer,
        label_selector,
        supports_odgm: true,
    }
}

pub fn descriptor_knative() -> TemplateDescriptor {
    TemplateDescriptor {
        id: "knative",
        managed: KNATIVE_MANAGED,
        readiness: KNATIVE_RULES,
        function_namer,
        label_selector,
        supports_odgm: true,
    }
}
