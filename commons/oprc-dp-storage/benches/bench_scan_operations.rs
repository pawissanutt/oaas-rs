use criterion::{criterion_group, criterion_main, Criterion};
use oprc_dp_storage::StorageBackend; // bring trait into scope
use tokio::runtime::Runtime;

#[path = "common/mod.rs"]
mod common;
use common::*;

const SMALL_VALUE_SIZE: usize = 100;
const MEDIUM_DATASET_SIZE: usize = 1_000;

fn bench_scan_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("scan_operations");

    // Memory
    let memory = create_memory_storage();
    rt.block_on(async {
        for i in 0..MEDIUM_DATASET_SIZE {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            memory.put(&key, value).await.unwrap();
        }
    });
    group.bench_function("scan_prefix_memory", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = memory.scan(b"benchmark_key_0001").await.unwrap();
            })
        });
    });
    group.bench_function("scan_range_memory", |b| {
        b.iter(|| {
            rt.block_on(async {
                let _ = memory
                    .scan_range(
                        generate_key(1000).into_vec()
                            ..generate_key(5000).into_vec(),
                    )
                    .await
                    .unwrap();
            })
        });
    });

    // Fjall
    #[cfg(feature = "fjall")]
    {
        let (fjall, _t) = create_fjall_storage();
        rt.block_on(async {
            for i in 0..MEDIUM_DATASET_SIZE {
                let key = generate_key(i);
                let value = generate_value(SMALL_VALUE_SIZE, i);
                fjall.put(&key, value).await.unwrap();
            }
        });
        group.bench_function("scan_prefix_fjall", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = fjall.scan(b"benchmark_key_0001").await.unwrap();
                })
            });
        });
        group.bench_function("scan_range_fjall", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = fjall
                        .scan_range(
                            generate_key(1000).into_vec()
                                ..generate_key(5000).into_vec(),
                        )
                        .await
                        .unwrap();
                })
            });
        });
    }

    // Redb
    #[cfg(feature = "redb")]
    {
        let (redb, _t) = create_redb_storage();
        rt.block_on(async {
            for i in 0..MEDIUM_DATASET_SIZE {
                let key = generate_key(i);
                let value = generate_value(SMALL_VALUE_SIZE, i);
                redb.put(&key, value).await.unwrap();
            }
        });
        group.bench_function("scan_prefix_redb", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = redb.scan(b"benchmark_key_0001").await.unwrap();
                })
            });
        });
        group.bench_function("scan_range_redb", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = redb
                        .scan_range(
                            generate_key(1000).into_vec()
                                ..generate_key(5000).into_vec(),
                        )
                        .await
                        .unwrap();
                })
            });
        });
    }

    // SkipList
    #[cfg(feature = "skiplist")]
    {
        let skiplist = create_skiplist_storage();
        rt.block_on(async {
            for i in 0..MEDIUM_DATASET_SIZE {
                let key = generate_key(i);
                let value = generate_value(SMALL_VALUE_SIZE, i);
                skiplist.put(&key, value).await.unwrap();
            }
        });
        group.bench_function("scan_prefix_skiplist", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = skiplist.scan(b"benchmark_key_0001").await.unwrap();
                })
            });
        });
        group.bench_function("scan_range_skiplist", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let _ = skiplist
                        .scan_range(
                            generate_key(1000).into_vec()
                                ..generate_key(5000).into_vec(),
                        )
                        .await
                        .unwrap();
                })
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_scan_operations);
criterion_main!(benches);
