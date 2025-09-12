use crate::crd::class_runtime::ClassRuntimeSpec;
use crate::crd::class_runtime::{
    ClassRuntimeStatus, Condition, ConditionStatus, ConditionType,
    FunctionStatus,
};
use crate::templates::manager::TemplateManager;

fn build_function_statuses(
    spec: &ClassRuntimeSpec,
    name: &str,
    template: &str,
) -> Option<Vec<FunctionStatus>> {
    if spec.functions.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(spec.functions.len());
    let base = TemplateManager::dns1035_safe(name);
    let multi = spec.functions.len() > 1;
    for (i, f) in spec.functions.iter().enumerate() {
        let svc = if multi {
            format!("{}-fn-{}", base, i)
        } else {
            base.clone()
        };
        let port = f
            .provision_config
            .as_ref()
            .and_then(|p| p.port)
            .unwrap_or(8080);
        let predicted_url = format!("http://{}:{}/", svc, port);
        let key = if f.function_key.trim().is_empty() {
            svc.clone()
        } else {
            f.function_key.clone()
        };
        out.push(FunctionStatus {
            function_key: key,
            service: svc,
            port,
            predicted_url,
            observed_url: None,
            template: template.to_string(),
            ready: None,
            reason: None,
            message: None,
            last_transition_time: None,
        });
    }
    Some(out)
}

pub fn progressing(
    now: String,
    generation: Option<i64>,
    spec: &ClassRuntimeSpec,
    name: &str,
    template: &str,
) -> ClassRuntimeStatus {
    let functions = build_function_statuses(spec, name, template);
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
        routers: None,
        functions,
    }
}
