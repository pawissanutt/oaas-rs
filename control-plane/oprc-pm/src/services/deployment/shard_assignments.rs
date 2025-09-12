use oprc_grpc::types as grpc_types;

// Public so other modules can reuse
pub fn generate_shard_assignments_spec(
    partition_count: usize,
    replica_count: usize,
    members: &[u64],
) -> Vec<grpc_types::ShardAssignmentSpec> {
    use grpc_types::ShardAssignmentSpec;
    if partition_count == 0 || replica_count == 0 || members.is_empty() {
        return vec![];
    }
    let mut primary_counts = vec![0usize; members.len()];
    let mut next_shard_id: u64 = 1;
    let mut out = Vec::with_capacity(partition_count);
    for _p in 0..partition_count {
        let primary_index = (0..members.len())
            .min_by_key(|i| (primary_counts[*i], *i))
            .unwrap();
        primary_counts[primary_index] += 1;
        let mut replica_nodes = Vec::with_capacity(replica_count);
        for i in 0..replica_count {
            replica_nodes.push(members[(primary_index + i) % members.len()]);
        }
        let mut shard_ids = Vec::with_capacity(replica_count);
        for _r in 0..replica_count {
            shard_ids.push(next_shard_id);
            next_shard_id += 1;
        }
        let primary = shard_ids.first().cloned();
        out.push(ShardAssignmentSpec {
            primary,
            replica: replica_nodes,
            shard_ids,
        });
    }
    out
}
