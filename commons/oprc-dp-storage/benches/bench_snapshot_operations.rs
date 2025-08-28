use criterion::{criterion_group, criterion_main, Criterion};
use oprc_dp_storage::{SnapshotCapableStorage, StorageBackend};
use tokio::runtime::Runtime;

#[path = "common/mod.rs"]
mod common;
use common::*;

const SMALL_DATASET_SIZE: usize = 100;
const SMALL_VALUE_SIZE: usize = 100;

fn bench_snapshot_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("snapshot_operations");

    let memory = create_memory_storage();
    rt.block_on(async {
        for i in 0..SMALL_DATASET_SIZE {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            memory.put(&key, value).await.unwrap();
        }
    });
    group.bench_function("snapshot_create_memory", |b| {
        b.iter(|| rt.block_on(async { memory.create_snapshot().await.unwrap(); }));
    });

    #[cfg(feature = "fjall")]
    {
        let (fjall, _t) = create_fjall_storage();
        rt.block_on(async {
            for i in 0..SMALL_DATASET_SIZE {
                let key = generate_key(i);
                let value = generate_value(SMALL_VALUE_SIZE, i);
                fjall.put(&key, value).await.unwrap();
            }
        });
        group.bench_function("snapshot_create_fjall", |b| {
            b.iter(|| rt.block_on(async { fjall.create_snapshot().await.unwrap(); }));
        });
    }

    #[cfg(feature = "skiplist")]
    {
        let skiplist = create_skiplist_storage();
        rt.block_on(async {
            for i in 0..SMALL_DATASET_SIZE {
                let key = generate_key(i);
                let value = generate_value(SMALL_VALUE_SIZE, i);
                skiplist.put(&key, value).await.unwrap();
            }
        });
        group.bench_function("snapshot_create_skiplist", |b| {
            b.iter(|| rt.block_on(async { skiplist.create_snapshot().await.unwrap(); }));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_snapshot_operations);
criterion_main!(benches);
