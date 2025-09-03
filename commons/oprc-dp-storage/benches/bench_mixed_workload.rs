use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, criterion_group,
    criterion_main,
};
use oprc_dp_storage::{StorageBackend, StorageValue};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::runtime::Runtime;

#[path = "common/mod.rs"]
mod common;
use common::*;

const VALUE_SIZE: usize = 128;

fn bench_mixed_workload(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("mixed_workload");

    for &ops in &[1_000usize, 10_000] {
        group.throughput(Throughput::Elements(ops as u64));

        // Decide operation and data based on a probability p
        let mix = |p: f64,
                   rng: &mut StdRng,
                   i: usize|
         -> (bool, StorageValue, StorageValue) {
            if rng.random::<f64>() < p {
                (true, generate_key(i), generate_value(VALUE_SIZE, i))
            } else {
                (false, generate_key(i), StorageValue::from(Vec::<u8>::new()))
            }
        };

        // Memory
        let memory = create_memory_storage();
        group.bench_with_input(
            BenchmarkId::new("mixed_memory", ops),
            &ops,
            |b, &n| {
                b.iter_batched(
                    || {
                        let mut rng = StdRng::seed_from_u64(123);
                        (0..n)
                            .map(|i| mix(0.5, &mut rng, i))
                            .collect::<Vec<_>>()
                    },
                    |ops: Vec<(bool, StorageValue, StorageValue)>| {
                        rt.block_on(async {
                            for (is_put, k, v) in ops.into_iter() {
                                if is_put {
                                    memory.put(&k, v).await.unwrap();
                                } else {
                                    let _ = memory.get(&k).await.unwrap();
                                }
                            }
                        })
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // Fjall
        #[cfg(feature = "fjall")]
        {
            let (fjall, _t) = create_fjall_storage();
            group.bench_with_input(
                BenchmarkId::new("mixed_fjall", ops),
                &ops,
                |b, &n| {
                    b.iter_batched(
                        || {
                            let mut rng = StdRng::seed_from_u64(123);
                            (0..n)
                                .map(|i| mix(0.5, &mut rng, i))
                                .collect::<Vec<_>>()
                        },
                        |ops: Vec<(bool, StorageValue, StorageValue)>| {
                            rt.block_on(async {
                                for (is_put, k, v) in ops.into_iter() {
                                    if is_put {
                                        fjall.put(&k, v).await.unwrap();
                                    } else {
                                        let _ = fjall.get(&k).await.unwrap();
                                    }
                                }
                            })
                        },
                        BatchSize::SmallInput,
                    );
                },
            );

            let (fjall_tx, _t) = create_fjall_tx_storage();
            group.bench_with_input(
                BenchmarkId::new("mixed_fjall_tx", ops),
                &ops,
                |b, &n| {
                    b.iter_batched(
                        || {
                            let mut rng = StdRng::seed_from_u64(123);
                            (0..n)
                                .map(|i| mix(0.5, &mut rng, i))
                                .collect::<Vec<_>>()
                        },
                        |ops: Vec<(bool, StorageValue, StorageValue)>| {
                            rt.block_on(async {
                                for (is_put, k, v) in ops.into_iter() {
                                    if is_put {
                                        fjall_tx.put(&k, v).await.unwrap();
                                    } else {
                                        let _ = fjall_tx.get(&k).await.unwrap();
                                    }
                                }
                            })
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }

        // Redb
        #[cfg(feature = "redb")]
        {
            let (redb, _t) = create_redb_storage();
            group.bench_with_input(
                BenchmarkId::new("mixed_redb", ops),
                &ops,
                |b, &n| {
                    b.iter_batched(
                        || {
                            let mut rng = StdRng::seed_from_u64(123);
                            (0..n)
                                .map(|i| mix(0.5, &mut rng, i))
                                .collect::<Vec<_>>()
                        },
                        |ops: Vec<(bool, StorageValue, StorageValue)>| {
                            rt.block_on(async {
                                for (is_put, k, v) in ops.into_iter() {
                                    if is_put {
                                        redb.put(&k, v).await.unwrap();
                                    } else {
                                        let _ = redb.get(&k).await.unwrap();
                                    }
                                }
                            })
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }

        // SkipList
        #[cfg(feature = "skiplist")]
        {
            let skiplist = create_skiplist_storage();
            group.bench_with_input(
                BenchmarkId::new("mixed_skiplist", ops),
                &ops,
                |b, &n| {
                    b.iter_batched(
                        || {
                            let mut rng = StdRng::seed_from_u64(123);
                            (0..n)
                                .map(|i| mix(0.5, &mut rng, i))
                                .collect::<Vec<_>>()
                        },
                        |ops: Vec<(bool, StorageValue, StorageValue)>| {
                            rt.block_on(async {
                                for (is_put, k, v) in ops.into_iter() {
                                    if is_put {
                                        skiplist.put(&k, v).await.unwrap();
                                    } else {
                                        let _ = skiplist.get(&k).await.unwrap();
                                    }
                                }
                            })
                        },
                        BatchSize::SmallInput,
                    );
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_mixed_workload);
criterion_main!(benches);
