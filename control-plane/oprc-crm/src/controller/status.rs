use crate::crd::class_runtime::ClassRuntimeSpec;
use crate::crd::class_runtime::{
    ClassRuntimeStatus, Condition, ConditionStatus, ConditionType,
    FunctionStatus,
};

fn build_function_statuses(
    spec: &ClassRuntimeSpec,
    name: &str,
    namespace: &str,
    template: &str,
) -> Option<Vec<FunctionStatus>> {
    if spec.functions.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(spec.functions.len());
    // number of functions is available via spec; not needed here
    for (i, f) in spec.functions.iter().enumerate() {
        let svc = crate::routing::function_service_name(
            name,
            i,
            spec.functions.len(),
        );
        let predicted_url = if template.eq_ignore_ascii_case("knative")
            || template.eq_ignore_ascii_case("kn")
            || template.eq_ignore_ascii_case("knsvc")
        {
            // Provide FQDN so callers relying on fully qualified DNS succeed inside cluster
            crate::routing::function_service_url_knative_fqdn(
                name,
                namespace,
                i,
                spec.functions.len(),
            )
        } else {
            crate::routing::function_service_url_fqdn(
                name,
                namespace,
                i,
                spec.functions.len(),
            )
        };
        let port = 80;
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
    namespace: &str,
    template: &str,
) -> ClassRuntimeStatus {
    let functions = build_function_statuses(spec, name, namespace, template);
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
