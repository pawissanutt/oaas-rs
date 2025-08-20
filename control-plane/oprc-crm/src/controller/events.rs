use kube::runtime::events::{Event, EventType, Recorder};

use super::build_obj_ref;

pub const REASON_APPLIED: &str = "Applied";
pub const REASON_NFR_APPLIED: &str = "NFRApplied";

pub async fn emit_event(
    recorder: &Recorder,
    ns: &str,
    name: &str,
    uid: Option<&str>,
    reason: &str,
    action: &str,
    note: Option<String>,
) {
    let _ = recorder
        .publish(
            &Event {
                type_: EventType::Normal,
                reason: reason.into(),
                note,
                action: action.into(),
                secondary: None,
            },
            &build_obj_ref(ns, name, uid),
        )
        .await;
}
