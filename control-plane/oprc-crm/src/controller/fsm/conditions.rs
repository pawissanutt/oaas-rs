use super::evaluator::Phase;
use crate::crd::class_runtime::{Condition, ConditionStatus, ConditionType};
use chrono::Utc;

/// Build a canonical condition set from the FSM phase plus optional previous conditions we want to preserve (e.g., NfrObserved).
pub fn build_conditions(
    phase: Phase,
    prev: Option<&Vec<Condition>>,
) -> Vec<Condition> {
    let now = Utc::now().to_rfc3339();
    let mut out: Vec<Condition> = Vec::new();

    let (ptype, status, reason, message) = match phase {
        Phase::Pending => (
            ConditionType::Progressing,
            ConditionStatus::False,
            "Pending",
            "Awaiting apply",
        ),
        Phase::Applying => (
            ConditionType::Progressing,
            ConditionStatus::True,
            "Applying",
            "Applying resources",
        ),
        Phase::Progressing => (
            ConditionType::Progressing,
            ConditionStatus::True,
            "Progressing",
            "Waiting for readiness",
        ),
        Phase::Available => (
            ConditionType::Available,
            ConditionStatus::True,
            "Ready",
            "All readiness rules satisfied",
        ),
        Phase::Degraded => (
            ConditionType::Degraded,
            ConditionStatus::True,
            "Degraded",
            "One or more readiness rules failing beyond threshold",
        ),
        Phase::Deleting => (
            ConditionType::Progressing,
            ConditionStatus::False,
            "Deleting",
            "Deleting child resources",
        ),
    };
    out.push(Condition {
        type_: ptype,
        status,
        reason: Some(reason.into()),
        message: Some(message.into()),
        last_transition_time: Some(now.clone()),
    });

    // Preserve NfrObserved condition (and others we don't manage) if present and not superseded
    if let Some(prev_list) = prev {
        for c in prev_list {
            match c.type_ {
                ConditionType::NfrObserved => {
                    // push a clone but don't mutate lastTransitionTime
                    out.push(c.clone());
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_available_condition() {
        let conds = build_conditions(Phase::Available, None);
        assert!(
            conds
                .iter()
                .any(|c| matches!(c.type_, ConditionType::Available))
        );
    }

    #[test]
    fn preserves_nfr_observed() {
        let prev = vec![Condition {
            type_: ConditionType::NfrObserved,
            status: ConditionStatus::True,
            reason: Some("Observed".into()),
            message: None,
            last_transition_time: None,
        }];
        let conds = build_conditions(Phase::Progressing, Some(&prev));
        assert!(
            conds
                .iter()
                .any(|c| matches!(c.type_, ConditionType::NfrObserved))
        );
    }
}
