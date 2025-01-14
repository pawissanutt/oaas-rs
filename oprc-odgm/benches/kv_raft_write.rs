use bytes::Bytes;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use flare_pb::CreateCollectionRequest;
use flare_pb::ShardAssignment;
use oprc_odgm::{
    shard::{ObjectEntry, ObjectVal},
    ObjectDataGridManager,
};
use rand::Rng;
use std::{sync::Arc, time::Duration};

async fn run(odgm: Arc<ObjectDataGridManager>, size: usize) {
    let key = rand::random::<u64>();
    let value: Vec<u8> = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(size)
        .map(u8::from)
        .collect();
    let shard = odgm.get_shard("benches", &key.to_be_bytes()).await.unwrap();
    let mut entries = std::collections::BTreeMap::new();
    entries.insert(0 as u32, ObjectVal::Byte(Bytes::from(value)));
    let object = ObjectEntry { value: entries };
    shard.set(key, object).await.unwrap();
}

pub fn criterion_benchmark(c: &mut Criterion) {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .build()
        .unwrap();

    static KB: usize = 1024;

    let mut group = c.benchmark_group("write throughput");
    for size in [512, KB, 2 * KB, 4 * KB, 8 * KB, 16 * KB].iter() {
        let odgm = runtime.block_on(async {
            let conf = oprc_odgm::Config {
                node_id: Some(1),
                http_port: 8080,
                collection: None,
                members: Some("1".into()),
            };
            let odgm = oprc_odgm::start_raw_server(&conf).await.unwrap();

            let request = CreateCollectionRequest {
                name: "benches".into(),
                partition_count: 1,
                replica_count: 1,
                shard_assignments: vec![ShardAssignment {
                    primary: Some(1),
                    replica: vec![1],
                    ..Default::default()
                }],
                ..Default::default()
            };
            odgm.metadata_manager
                .create_collection(request)
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            odgm
        });
        group.throughput(criterion::Throughput::Elements(1));
        group.bench_with_input(
            BenchmarkId::new("write", size),
            &size,
            |b, &s| {
                b.to_async(&runtime).iter(|| run(odgm.clone(), *s));
            },
        );
        runtime.block_on(async {
            odgm.close().await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        })
    }
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
