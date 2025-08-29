use oprc_pb::{CreateCollectionRequest, FuncInvokeRoute, InvocationRoute};
use std::collections::{BTreeMap, HashMap};

use crate::crd::class_runtime::{
    FunctionRoute as CrdFunctionRoute, InvocationsSpec,
};

/// Helper to build a CreateCollectionRequest consistent with docker-compose ODGM_COLLECTION JSON.
/// Accepts optional `invocations` and `options` from CRD spec to reduce caller boilerplate.
pub fn build_collection_request(
    name: &str,
    partition_count: i32,
    replica_count: i32,
    shard_type: &str,
    invocations: Option<&InvocationsSpec>,
    options: Option<&BTreeMap<String, String>>,
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
            },
        ) in inv.fn_routes.iter()
        {
            routes.insert(
                id.clone(),
                FuncInvokeRoute {
                    url: url.clone(),
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

    CreateCollectionRequest {
        name: name.to_string(),
        partition_count,
        replica_count,
        shard_assignments: vec![],
        shard_type: shard_type.to_string(),
        options: options_map,
        invocations: invoc_pb,
    }
}

/// Minimal MST example with a single echo route.
pub fn minimal_mst_with_echo(name: &str) -> CreateCollectionRequest {
    // Provide a tiny inline invocations spec with a single echo route
    let mut fn_routes: BTreeMap<String, CrdFunctionRoute> = BTreeMap::new();
    fn_routes.insert(
        "echo".to_string(),
        CrdFunctionRoute {
            url: "http://echo-fn".to_string(),
            stateless: Some(true),
            standby: Some(false),
            active_group: vec![],
        },
    );
    let inv = InvocationsSpec {
        fn_routes,
        disabled_fn: vec![],
    };
    build_collection_request(name, 1, 1, "mst", Some(&inv), None)
}
