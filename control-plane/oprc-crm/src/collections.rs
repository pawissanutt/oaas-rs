use oprc_grpc::{CreateCollectionRequest, FuncInvokeRoute, InvocationRoute};
use std::collections::{BTreeMap, HashMap};
use tracing::warn;

use crate::crd::class_runtime::{
    FunctionRoute as CrdFunctionRoute, InvocationsSpec, ShardAssignmentSpec,
};

/// Helper to build a CreateCollectionRequest consistent with docker-compose ODGM_COLLECTION JSON.
/// Accepts optional `invocations` and `options` from CRD spec to reduce caller boilerplate.
pub fn build_collection_request(
    name: &str,
    namespace: &str,
    partition_count: i32,
    replica_count: i32,
    shard_type: &str,
    invocations: Option<&InvocationsSpec>,
    options: Option<&BTreeMap<String, String>>,
    assignments: Option<&[ShardAssignmentSpec]>,
) -> CreateCollectionRequest {
    // Convert options to HashMap as required by protobuf type
    let mut options_map: HashMap<String, String> = HashMap::new();
    if let Some(opts) = options {
        options_map.extend(opts.iter().map(|(k, v)| (k.clone(), v.clone())));
    }

    // Build function invocation routes if provided
    let invoc_pb = invocations.map(|inv| {
        let mut routes: HashMap<String, FuncInvokeRoute> = HashMap::new();
        for (
            id,
            CrdFunctionRoute {
                url,
                stateless,
                standby,
                active_group,
                function_key: _,
            },
        ) in inv.fn_routes.iter()
        {
            // Normalize URL to FQDN form when it looks like a short internal service name.
            // Patterns we upgrade: http://service(/|:port/) or http://service-fn-#(:80)?/ .
            // We avoid modifying if already contains a dot (likely FQDN or external host).
            let mut norm_url = url.clone();
            if let Some(stripped) = url.strip_prefix("http://") {
                if !stripped.contains('.') {
                    // Extract service token up to first '/'
                    let (svc_part, rest) = stripped
                        .split_once('/')
                        .map(|(a, b)| (a.to_string(), format!("/{}", b)))
                        .unwrap_or_else(|| {
                            (
                                stripped.trim_end_matches('/').to_string(),
                                "/".to_string(),
                            )
                        });
                    // Remove any :80 suffix for canonicalization
                    let svc_core = svc_part.trim_end_matches('/');
                    let svc_core =
                        svc_core.strip_suffix(":80").unwrap_or(svc_core);
                    norm_url = format!(
                        "http://{}.{}.svc.cluster.local{}",
                        svc_core,
                        namespace,
                        if rest == "" { "/".into() } else { rest }
                    );
                    if !norm_url.ends_with('/') {
                        norm_url.push('/');
                    }
                }
            }
            routes.insert(
                id.clone(),
                FuncInvokeRoute {
                    url: norm_url,
                    stateless: stateless.unwrap_or(true),
                    standby: standby.unwrap_or(false),
                    active_group: active_group.clone(),
                },
            );
        }
        InvocationRoute {
            fn_routes: routes,
            disabled_fn: inv.disabled_fn.clone(),
        }
    });

    // Map provided assignments (one per partition) to protobuf if present.
    let shard_assignments = assignments
        .and_then(|list| {
            if list.is_empty() {
                return None;
            }
            if !validate_assignments(list, partition_count, replica_count) {
                warn!(collection=%name, "Invalid shard assignments provided; falling back to ODGM auto-generation");
                return None;
            }
            Some(
                list.iter()
                    .map(|a| oprc_grpc::ShardAssignment {
                        primary: a.primary,
                        replica: a.replica.clone(),
                        shard_ids: a.shard_ids.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();

    CreateCollectionRequest {
        name: name.to_string(),
        partition_count,
        replica_count,
        shard_assignments,
        shard_type: shard_type.to_string(),
        options: options_map,
        invocations: invoc_pb,
    }
}

fn validate_assignments(
    list: &[ShardAssignmentSpec],
    partition_count: i32,
    replica_count: i32,
) -> bool {
    if partition_count <= 0 || replica_count <= 0 {
        return false;
    }
    if list.len() != partition_count as usize {
        warn!(expected=%partition_count, actual=%list.len(), "partition count mismatch in assignments");
        return false;
    }
    let mut seen_shards = std::collections::BTreeSet::new();
    for (idx, a) in list.iter().enumerate() {
        if a.replica.len() != replica_count as usize {
            warn!(partition=%idx, expected=%replica_count, actual=%a.replica.len(), "replica owner count mismatch");
            return false;
        }
        if a.shard_ids.len() != replica_count as usize {
            warn!(partition=%idx, expected=%replica_count, actual=%a.shard_ids.len(), "shard id count mismatch");
            return false;
        }
        if let Some(p) = a.primary {
            if !a.shard_ids.contains(&p) {
                warn!(partition=%idx, primary=%p, "primary not in shard_ids");
                return false;
            }
        }
        for sid in &a.shard_ids {
            if !seen_shards.insert(*sid) {
                warn!(partition=%idx, shard_id=%sid, "duplicate shard id across partitions");
                return false;
            }
        }
    }
    true
}
