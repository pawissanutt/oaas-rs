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

    // Preserve NfrObserved and Telemetry conditions (and others we don't manage) if present and not superseded
    if let Some(prev_list) = prev {
        for c in prev_list {
            match c.type_ {
                ConditionType::NfrObserved
                | ConditionType::TelemetryConfigured
                | ConditionType::TelemetryDegraded => {
                    // push a clone but don't mutate lastTransitionTime
                    out.push(c.clone());
                }
                _ => {}
            }
        }
    }
    out
}

/// Build a TelemetryConfigured condition when telemetry is enabled.
pub fn build_telemetry_configured_condition(
    enabled: bool,
    endpoint_present: bool,
) -> Condition {
    let now = chrono::Utc::now().to_rfc3339();
    if enabled && endpoint_present {
        Condition {
            type_: ConditionType::TelemetryConfigured,
            status: ConditionStatus::True,
            reason: Some("TelemetryEnabled".into()),
            message: Some("Telemetry is enabled with valid endpoint".into()),
            last_transition_time: Some(now),
        }
    } else if enabled {
        // Enabled but no endpoint - degraded
        Condition {
            type_: ConditionType::TelemetryDegraded,
            status: ConditionStatus::True,
            reason: Some("NoEndpoint".into()),
            message: Some(
                "Telemetry enabled but no OTLP endpoint configured".into(),
            ),
            last_transition_time: Some(now),
        }
    } else {
        // Telemetry not enabled - configured as disabled
        Condition {
            type_: ConditionType::TelemetryConfigured,
            status: ConditionStatus::False,
            reason: Some("TelemetryDisabled".into()),
            message: Some("Telemetry is disabled".into()),
            last_transition_time: Some(now),
        }
    }
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

    #[test]
    fn builds_telemetry_configured_when_enabled_with_endpoint() {
        let cond = build_telemetry_configured_condition(true, true);
        assert!(matches!(cond.type_, ConditionType::TelemetryConfigured));
        assert!(matches!(cond.status, ConditionStatus::True));
        assert_eq!(cond.reason, Some("TelemetryEnabled".into()));
    }

    #[test]
    fn builds_telemetry_degraded_when_enabled_no_endpoint() {
        let cond = build_telemetry_configured_condition(true, false);
        assert!(matches!(cond.type_, ConditionType::TelemetryDegraded));
        assert!(matches!(cond.status, ConditionStatus::True));
        assert_eq!(cond.reason, Some("NoEndpoint".into()));
    }

    #[test]
    fn builds_telemetry_configured_false_when_disabled() {
        let cond = build_telemetry_configured_condition(false, true);
        assert!(matches!(cond.type_, ConditionType::TelemetryConfigured));
        assert!(matches!(cond.status, ConditionStatus::False));
        assert_eq!(cond.reason, Some("TelemetryDisabled".into()));
    }

    #[test]
    fn preserves_telemetry_conditions() {
        let prev = vec![
            Condition {
                type_: ConditionType::TelemetryConfigured,
                status: ConditionStatus::True,
                reason: Some("TelemetryEnabled".into()),
                message: None,
                last_transition_time: None,
            },
            Condition {
                type_: ConditionType::NfrObserved,
                status: ConditionStatus::True,
                reason: Some("Observed".into()),
                message: None,
                last_transition_time: None,
            },
        ];
        let conds = build_conditions(Phase::Available, Some(&prev));

        // Should have Available + TelemetryConfigured + NfrObserved
        assert!(
            conds
                .iter()
                .any(|c| matches!(c.type_, ConditionType::Available))
        );
        assert!(
            conds
                .iter()
                .any(|c| matches!(c.type_, ConditionType::TelemetryConfigured))
        );
        assert!(
            conds
                .iter()
                .any(|c| matches!(c.type_, ConditionType::NfrObserved))
        );
    }
}
