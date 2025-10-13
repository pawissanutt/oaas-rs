use oprc_grpc::{CreateCollectionRequest, FuncInvokeRoute, InvocationRoute};
use std::collections::HashMap;

/// Build a CreateCollectionRequest matching the docker-compose ODGM_COLLECTION format.
/// fn_routes: slice of (route_id, url, stateless, standby)
pub fn build_collection_request(
    name: &str,
    partition_count: i32,
    replica_count: i32,
    shard_type: &str,
    fn_routes: &[(&str, &str, bool, bool)],
) -> CreateCollectionRequest {
    let mut routes = HashMap::new();
    for (id, url, stateless, standby) in fn_routes.iter() {
        routes.insert(
            (*id).to_string(),
            FuncInvokeRoute {
                url: (*url).to_string(),
                stateless: *stateless,
                standby: *standby,
                active_group: vec![],
            },
        );
    }
    CreateCollectionRequest {
        name: name.to_string(),
        partition_count,
        replica_count,
        shard_assignments: vec![],
        shard_type: shard_type.to_string(),
        options: HashMap::new(),
        invocations: Some(InvocationRoute {
            fn_routes: routes,
            disabled_fn: vec![],
        }),
    }
}

pub fn minimal_mst_with_echo(name: &str) -> CreateCollectionRequest {
    build_collection_request(
        name,
        1,
        1,
        "mst",
        &[("echo", "http://echo-fn", true, false)],
    )
}
