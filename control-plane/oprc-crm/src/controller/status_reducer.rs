use crate::crd::class_runtime::{
    ClassRuntimeStatus, Condition, ConditionType, FunctionStatus,
};

/// Merge `desired` status into `current`, preserving fields that should not be
/// clobbered by reconcile phases (e.g., analyzer/enforcer outputs) unless the
/// desired explicitly sets them. Returns a new status object.
pub fn merge_status(
    current: Option<&ClassRuntimeStatus>,
    mut desired: ClassRuntimeStatus,
) -> ClassRuntimeStatus {
    if let Some(cur) = current {
        // Preserve analyzer/enforcer fields when desired doesn't touch them
        if desired.nfr_recommendations.is_none() {
            desired.nfr_recommendations = cur.nfr_recommendations.clone();
        }
        if desired.last_applied_recommendations.is_none() {
            desired.last_applied_recommendations =
                cur.last_applied_recommendations.clone();
        }
        if desired.last_applied_at.is_none() {
            desired.last_applied_at = cur.last_applied_at.clone();
        }
        // Preserve routers unless new discovery populated
        if desired.routers.is_none() {
            desired.routers = cur.routers.clone();
        }
        // Preserve resource refs if not populated this cycle
        if desired.resource_refs.is_none() {
            desired.resource_refs = cur.resource_refs.clone();
        }

        // Merge function readiness: keep existing readiness-related fields
        // unless the desired explicitly sets them.
        if let Some(mut desired_funcs) = desired.functions.take() {
            if let Some(cur_funcs) = cur.functions.as_ref() {
                for d in &mut desired_funcs {
                    if let Some(c) = cur_funcs
                        .iter()
                        .find(|f| f.function_key == d.function_key)
                    {
                        if d.ready.is_none() {
                            d.ready = c.ready;
                        }
                        if d.reason.is_none() {
                            d.reason = c.reason.clone();
                        }
                        if d.message.is_none() {
                            d.message = c.message.clone();
                        }
                        if d.last_transition_time.is_none() {
                            d.last_transition_time =
                                c.last_transition_time.clone();
                        }
                        // If observed_url was set previously (e.g., Knative), preserve it
                        if d.observed_url.is_none() {
                            d.observed_url = c.observed_url.clone();
                        }
                    }
                }
            }
            desired.functions = Some(desired_funcs);
        } else {
            desired.functions = cur.functions.clone();
        }

        // Merge conditions: upsert by type, preserving other condition types.
        desired.conditions = Some(upsert_conditions(
            cur.conditions.as_ref().unwrap_or(&vec![]),
            desired.conditions.unwrap_or_default(),
        ));
    }
    desired
}

fn upsert_conditions(
    existing: &Vec<Condition>,
    incoming: Vec<Condition>,
) -> Vec<Condition> {
    let mut out: Vec<Condition> = existing.clone();
    for inc in incoming {
        if let Some(idx) = out.iter().position(|c| {
            std::mem::discriminant(&c.type_)
                == std::mem::discriminant(&inc.type_)
        }) {
            out[idx] = inc;
        } else {
            out.push(inc);
        }
    }
    // Optional: keep a stable order by type name to reduce churn
    out.sort_by(|a, b| cond_rank(&a.type_).cmp(&cond_rank(&b.type_)));
    out
}

fn cond_rank(t: &ConditionType) -> u8 {
    match t {
        ConditionType::Available => 0,
        ConditionType::Progressing => 1,
        ConditionType::Degraded => 2,
        ConditionType::NfrObserved => 3,
        ConditionType::Unknown => 250,
    }
}

#[allow(dead_code)]
fn _dummy_use(_fs: &FunctionStatus) {}
