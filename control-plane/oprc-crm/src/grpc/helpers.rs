use crate::crd::deployment_record::{
    ConditionStatus, ConditionType, DeploymentRecord, DeploymentRecordSpec,
    DeploymentRecordStatus,
};
use k8s_openapi::api::core::v1::Node;
use kube::Resource;
use kube::ResourceExt;
use std::time::Duration;
use tonic::metadata::MetadataMap;
use tonic::{Response, Status, metadata::MetadataValue};

pub const LABEL_DEPLOYMENT_ID: &str = "oaas.io/deployment-id";
pub const ANNO_CORRELATION_ID: &str = "oaas.io/correlation-id";

pub fn sanitize_name(id: &str) -> String {
    let mut s = id.to_ascii_lowercase();
    s = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();
    s.trim_matches('-').to_string()
}

pub fn internal<E: std::fmt::Display>(e: E) -> Status {
    Status::internal(e.to_string())
}

pub fn attach_corr<T>(
    mut resp: Response<T>,
    corr: &Option<String>,
) -> Response<T> {
    if let Some(c) = corr {
        if let Ok(v) = MetadataValue::try_from(c.clone()) {
            resp.metadata_mut().insert("x-correlation-id", v);
        }
    }
    resp
}

pub fn build_deployment_record(
    name: &str,
    deployment_id: &str,
    corr: Option<String>,
) -> DeploymentRecord {
    let mut dr = DeploymentRecord::new(
        name,
        DeploymentRecordSpec {
            selected_template: None,
            addons: None,
            odgm_config: None,
            functions: vec![],
            nfr_requirements: None,
            nfr: None,
        },
    );
    let labels = dr.meta_mut().labels.get_or_insert_with(Default::default);
    labels.insert(LABEL_DEPLOYMENT_ID.into(), deployment_id.to_string());
    if let Some(c) = corr {
        let ann = dr
            .meta_mut()
            .annotations
            .get_or_insert_with(Default::default);
        ann.insert(ANNO_CORRELATION_ID.into(), c);
    }
    dr
}

pub fn summarize_status(s: &DeploymentRecordStatus) -> String {
    if let Some(conds) = &s.conditions {
        if conds.iter().any(|c| {
            matches!(c.type_, ConditionType::Available)
                && matches!(c.status, ConditionStatus::True)
        }) {
            return "Available".to_string();
        }
        if let Some(c) = conds.iter().find(|c| {
            matches!(c.type_, ConditionType::Degraded)
                && matches!(c.status, ConditionStatus::True)
        }) {
            let mut msg = "Degraded".to_string();
            if let Some(r) = &c.reason {
                msg.push_str(&format!(" ({})", r));
            }
            if let Some(m) = &c.message {
                msg.push_str(&format!(": {}", m));
            }
            return msg;
        }
        if conds.iter().any(|c| {
            matches!(c.type_, ConditionType::Progressing)
                && matches!(c.status, ConditionStatus::True)
        }) {
            return "Progressing".to_string();
        }
    }
    if let Some(p) = &s.phase {
        return p.clone();
    }
    if let Some(m) = &s.message {
        return m.clone();
    }
    "found".to_string()
}

pub fn summarized_enum(
    s: &DeploymentRecordStatus,
) -> oprc_grpc::proto::deployment::SummarizedStatus {
    use oprc_grpc::proto::deployment::SummarizedStatus as S;
    if let Some(conds) = &s.conditions {
        if conds.iter().any(|c| {
            matches!(c.type_, ConditionType::Available)
                && matches!(c.status, ConditionStatus::True)
        }) {
            return S::Running;
        }
        if conds.iter().any(|c| {
            matches!(c.type_, ConditionType::Degraded)
                && matches!(c.status, ConditionStatus::True)
        }) {
            return S::Degraded;
        }
        if conds.iter().any(|c| {
            matches!(c.type_, ConditionType::Progressing)
                && matches!(c.status, ConditionStatus::True)
        }) {
            return S::Progressing;
        }
    }
    S::SummaryUnknown
}

pub fn parse_grpc_timeout(meta: &MetadataMap) -> Option<Duration> {
    let val = meta.get("grpc-timeout")?.to_str().ok()?;
    if val.is_empty() {
        return None;
    }
    let (num_part, unit_part) = val.split_at(val.len().saturating_sub(1));
    let n: u64 = num_part.parse().ok()?;
    match unit_part {
        "H" => Some(Duration::from_secs(n.saturating_mul(3600))),
        "M" => Some(Duration::from_secs(n.saturating_mul(60))),
        "S" => Some(Duration::from_secs(n)),
        "m" => Some(Duration::from_millis(n)),
        "u" => Some(Duration::from_micros(n)),
        "n" => Some(Duration::from_nanos(n)),
        _ => None,
    }
}

pub fn validate_name(name: &str) -> Result<(), Status> {
    if name.is_empty() {
        return Err(Status::invalid_argument(
            "invalid deployment_id: empty after sanitization",
        ));
    }
    if name.len() > 253 {
        return Err(Status::invalid_argument(
            "invalid deployment_id: length exceeds 253 characters",
        ));
    }
    Ok(())
}

pub fn validate_existing_id(
    existing: &DeploymentRecord,
    requested_id: &str,
) -> Result<(), Status> {
    let labels = existing.metadata.labels.as_ref();
    let Some(lbls) = labels else {
        return Err(Status::already_exists(
            "resource name in use by a different deployment (no label)",
        ));
    };
    match lbls.get(LABEL_DEPLOYMENT_ID) {
        Some(v) if v == requested_id => Ok(()),
        _ => Err(Status::already_exists(
            "resource name in use by a different deployment",
        )),
    }
}

pub fn map_crd_to_proto(
    dr: &DeploymentRecord,
) -> oprc_grpc::proto::deployment::DeploymentUnit {
    use oprc_grpc::proto::{
        common as oaas_common, deployment as oaas_deployment,
    };

    let key = dr
        .metadata
        .labels
        .as_ref()
        .and_then(|m| m.get(LABEL_DEPLOYMENT_ID))
        .cloned()
        .unwrap_or_else(|| dr.name_any());

    let created_at = dr.metadata.creation_timestamp.as_ref().and_then(|t| {
        // kube::Time wraps chrono::DateTime in .0
        let ts_secs = t.0.timestamp();
        let ts_nanos = t.0.timestamp_subsec_nanos() as i32;
        Some(oaas_common::Timestamp {
            seconds: ts_secs,
            nanos: ts_nanos,
        })
    });

    // DeploymentUnit response does not include summarized_status or resource_refs

    oaas_deployment::DeploymentUnit {
        id: key,
        package_name: "".into(),
        class_key: dr.name_any(),
        functions: vec![],
        // best-effort: use namespace as env hint
        target_env: dr.namespace().unwrap_or_default(),
        created_at,
        odgm_config: None,
    }
}

/// Count total nodes and ready nodes from a slice of k8s `Node` objects.
pub fn count_nodes(nodes: &[Node]) -> (u32, u32) {
    let total = nodes.len() as u32;
    let mut ready = 0u32;
    for n in nodes.iter() {
        if let Some(status) = &n.status {
            if let Some(conds) = &status.conditions {
                for c in conds {
                    if c.type_ == "Ready" && c.status == "True" {
                        ready += 1;
                        break;
                    }
                }
            }
        }
    }
    (total, ready)
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::Node;
    use serde_json::json;

    #[test]
    fn test_count_nodes_ready_and_total() {
        let n1: Node = serde_json::from_value(json!({
            "metadata": {},
            "status": { "conditions": [{ "type": "Ready", "status": "True" }] }
        }))
        .unwrap();
        let n2: Node = serde_json::from_value(json!({
            "metadata": {},
            "status": { "conditions": [{ "type": "Ready", "status": "False" }] }
        }))
        .unwrap();

        let (total, ready) = count_nodes(&[n1, n2]);
        assert_eq!(total, 2);
        assert_eq!(ready, 1);
    }
}
