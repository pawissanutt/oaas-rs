//! Helper functions for network message parsing

use crate::identity::{ObjectIdentity, normalize_object_id};
use oprc_dp_storage::StorageValue;

pub(super) fn split_path_segments(expr: &str) -> Vec<&str> {
    expr.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

pub(super) fn parse_object_identity_segment(
    segment: &str,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    if let Ok(num) = segment.parse::<u64>() {
        return Some(ObjectIdentity::Numeric(num));
    }
    match normalize_object_id(segment, max_string_id_len) {
        Ok(norm) => Some(ObjectIdentity::Str(norm)),
        Err(e) => {
            tracing::debug!(
                "Failed to normalize string object id '{}': {}",
                segment,
                e
            );
            None
        }
    }
}

pub(super) fn extract_identity_from_expr(
    expr: &str,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    let segments = split_path_segments(expr);
    let objects_pos = segments.iter().rposition(|seg| *seg == "objects")?;
    let mut idx = objects_pos + 1;
    if idx >= segments.len() {
        return None;
    }
    let mut candidate = segments[idx];
    if matches!(candidate, "get" | "set" | "batch-set") {
        idx += 1;
        if idx >= segments.len() {
            return None;
        }
        candidate = segments[idx];
    }
    parse_object_identity_segment(candidate, max_string_id_len)
}

pub(super) fn parse_entry_request(
    expr: &str,
    max_string_id_len: usize,
) -> Option<(ObjectIdentity, String)> {
    let segments = split_path_segments(expr);
    let objects_pos = segments.iter().rposition(|seg| *seg == "objects")?;
    let obj_idx = objects_pos + 1;
    if obj_idx + 2 >= segments.len() {
        return None;
    }
    if segments[obj_idx + 1] != "entries" {
        return None;
    }
    let identity = parse_object_identity_segment(segments[obj_idx], max_string_id_len)?;
    let entry_key = segments[obj_idx + 2].to_string();
    Some((identity, entry_key))
}

pub(super) fn parse_batch_set_identity(
    expr: &str,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    let segments = split_path_segments(expr);
    let objects_pos = segments.iter().rposition(|seg| *seg == "objects")?;
    let obj_idx = objects_pos + 1;
    if obj_idx + 1 >= segments.len() {
        return None;
    }
    if segments[obj_idx + 1] != "batch-set" {
        return None;
    }
    parse_object_identity_segment(segments[obj_idx], max_string_id_len)
}

pub(super) fn parse_identity_from_query(
    shard_id: u64,
    query: &zenoh::query::Query,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    if let Some(identity) = extract_identity_from_expr(query.key_expr().as_str(), max_string_id_len)
    {
        return Some(identity);
    }

    let selector = query.selector();
    let parameters = selector.parameters();
    if let Some(oid_param) = parameters.get("oid") {
        if let Ok(oid) = oid_param.parse::<u64>() {
            return Some(ObjectIdentity::Numeric(oid));
        }
    }
    if let Some(oid_str) = parameters.get("oid_str") {
        match normalize_object_id(oid_str, max_string_id_len) {
            Ok(norm) => return Some(ObjectIdentity::Str(norm)),
            Err(e) => {
                tracing::debug!(
                    "(shard={}) Failed to normalize object_id_str '{}': {}",
                    shard_id,
                    oid_str,
                    e
                );
            }
        }
    }

    tracing::debug!(
        "(shard={}) Failed to parse object identity from key '{}'",
        shard_id,
        query.key_expr()
    );
    None
}

// Legacy storage key helper for numeric identity; string variant unused in granular path.
pub(super) fn storage_key_for_identity(identity: &ObjectIdentity) -> StorageValue {
    match identity {
        ObjectIdentity::Numeric(oid) => StorageValue::from(oid.to_be_bytes().to_vec()),
        ObjectIdentity::Str(sid) => StorageValue::from(sid.as_bytes()),
    }
}
