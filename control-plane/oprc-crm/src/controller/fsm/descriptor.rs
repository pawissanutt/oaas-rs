#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ManagedKind {
    Deployment,
    Service,
    KnativeService,
    OdgmDeployment,
    OdgmService,
    Hpa,
}

#[derive(Clone, Debug)]
pub struct ReadinessResult {
    pub ready: bool,
    pub reason: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone)]
pub struct ReadinessRule {
    pub kind: ManagedKind,
    pub contributes_to_available: bool,
    pub evaluator:
        fn(&crate::controller::fsm::observed::Observed) -> ReadinessResult,
}

#[derive(Clone)]
pub struct TemplateDescriptor {
    pub id: &'static str,
    pub managed: &'static [ManagedKind],
    pub readiness: &'static [ReadinessRule],
    pub function_namer: fn(class: &str, idx: usize, total: usize) -> String,
    pub label_selector: fn(class: &str) -> (String, String),
    pub supports_odgm: bool,
}
