use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use oprc_dp_storage::StorageBackend;
use rand::Rng;
use tokio::runtime::Runtime;

#[path = "common/mod.rs"]
mod common;
use common::*;

const OPS_PER_TASK: usize = 50;

fn bench_concurrent_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("concurrent_operations");

    for &level in &[2usize, 4, 8, 16] {
        group.throughput(Throughput::Elements((level * OPS_PER_TASK) as u64));

        // Memory backend
        let memory = create_memory_storage();
        rt.block_on(async {
            for i in 0..1000 {
                let _ =
                    memory.put(&generate_key(i), generate_value(100, i)).await;
            }
        });
        group.bench_with_input(
            BenchmarkId::new("memory_mixed_concurrent", level),
            &level,
            |b, &concurrency| {
                b.iter(|| {
                    rt.block_on(async {
                        let mut handles = Vec::with_capacity(concurrency);
                        for t in 0..concurrency {
                            let store = memory.clone();
                            handles.push(tokio::spawn(async move {
                                for op in 0..OPS_PER_TASK {
                                    let is_read = {
                                        let mut r = rand::rng();
                                        r.random::<f32>() < 0.7
                                    };
                                    if is_read {
                                        let key_index = {
                                            let mut r = rand::rng();
                                            r.random::<u32>() as usize % 1000
                                        };
                                        let k = generate_key(key_index);
                                        let _ = store.get(&k).await.unwrap();
                                    } else {
                                        let k = generate_key(
                                            50_000 + t * 1000 + op,
                                        );
                                        let v = generate_value(100, op);
                                        store.put(&k, v).await.unwrap();
                                    }
                                }
                            }));
                        }
                        for h in handles {
                            h.await.unwrap();
                        }
                    });
                });
            },
        );

        // Fjall backend
        #[cfg(feature = "fjall")]
        {
            let (fjall, _t) = create_fjall_storage();
            rt.block_on(async {
                for i in 0..1000 {
                    let _ = fjall
                        .put(&generate_key(i), generate_value(100, i))
                        .await;
                }
            });
            group.bench_with_input(
                BenchmarkId::new("fjall_mixed_concurrent", level),
                &level,
                |b, &concurrency| {
                    b.iter(|| {
                        rt.block_on(async {
                            let mut handles = Vec::with_capacity(concurrency);
                            for t in 0..concurrency {
                                let store = fjall.clone();
                                handles.push(tokio::spawn(async move {
                                    for op in 0..OPS_PER_TASK {
                                        let is_read = {
                                            let mut r = rand::rng();
                                            r.random::<f32>() < 0.7
                                        };
                                        if is_read {
                                            let key_index = {
                                                let mut r = rand::rng();
                                                r.random::<u32>() as usize
                                                    % 1000
                                            };
                                            let k = generate_key(key_index);
                                            let _ =
                                                store.get(&k).await.unwrap();
                                        } else {
                                            let k = generate_key(
                                                60_000 + t * 1000 + op,
                                            );
                                            let v = generate_value(100, op);
                                            store.put(&k, v).await.unwrap();
                                        }
                                    }
                                }));
                            }
                            for h in handles {
                                h.await.unwrap();
                            }
                        });
                    });
                },
            );

            let (fjall_tx, _t) = create_fjall_tx_storage();
            rt.block_on(async {
                for i in 0..1000 {
                    let _ = fjall_tx
                        .put(&generate_key(i), generate_value(100, i))
                        .await;
                }
            });
            group.bench_with_input(
                BenchmarkId::new("fjall_tx_mixed_concurrent", level),
                &level,
                |b, &concurrency| {
                    b.iter(|| {
                        rt.block_on(async {
                            let mut handles = Vec::with_capacity(concurrency);
                            for t in 0..concurrency {
                                let store = fjall_tx.clone();
                                handles.push(tokio::spawn(async move {
                                    for op in 0..OPS_PER_TASK {
                                        let is_read = {
                                            let mut r = rand::rng();
                                            r.random::<f32>() < 0.7
                                        };
                                        if is_read {
                                            let key_index = {
                                                let mut r = rand::rng();
                                                r.random::<u32>() as usize
                                                    % 1000
                                            };
                                            let k = generate_key(key_index);
                                            let _ =
                                                store.get(&k).await.unwrap();
                                        } else {
                                            let k = generate_key(
                                                60_000 + t * 1000 + op,
                                            );
                                            let v = generate_value(100, op);
                                            store.put(&k, v).await.unwrap();
                                        }
                                    }
                                }));
                            }
                            for h in handles {
                                h.await.unwrap();
                            }
                        });
                    });
                },
            );
        }

        // SkipList backend
        #[cfg(feature = "skiplist")]
        {
            let skiplist = create_skiplist_storage();
            rt.block_on(async {
                for i in 0..1000 {
                    let _ = skiplist
                        .put(&generate_key(i), generate_value(100, i))
                        .await;
                }
            });
            group.bench_with_input(
                BenchmarkId::new("skiplist_mixed_concurrent", level),
                &level,
                |b, &concurrency| {
                    b.iter(|| {
                        rt.block_on(async {
                            let mut handles = Vec::with_capacity(concurrency);
                            for t in 0..concurrency {
                                let store = skiplist.clone();
                                handles.push(tokio::spawn(async move {
                                    for op in 0..OPS_PER_TASK {
                                        let is_read = {
                                            let mut r = rand::rng();
                                            r.random::<f32>() < 0.7
                                        };
                                        if is_read {
                                            let key_index = {
                                                let mut r = rand::rng();
                                                r.random::<u32>() as usize
                                                    % 1000
                                            };
                                            let k = generate_key(key_index);
                                            let _ =
                                                store.get(&k).await.unwrap();
                                        } else {
                                            let k = generate_key(
                                                70_000 + t * 1000 + op,
                                            );
                                            let v = generate_value(100, op);
                                            store.put(&k, v).await.unwrap();
                                        }
                                    }
                                }));
                            }
                            for h in handles {
                                h.await.unwrap();
                            }
                        });
                    });
                },
            );
        }

        // Redb backend
        #[cfg(feature = "redb")]
        {
            let (redb, _t) = create_redb_storage();
            rt.block_on(async {
                for i in 0..1000 {
                    let _ = redb
                        .put(&generate_key(i), generate_value(100, i))
                        .await;
                }
            });
            group.bench_with_input(
                BenchmarkId::new("redb_mixed_concurrent", level),
                &level,
                |b, &concurrency| {
                    b.iter(|| {
                        rt.block_on(async {
                            let mut handles = Vec::with_capacity(concurrency);
                            for t in 0..concurrency {
                                let store = redb.clone();
                                handles.push(tokio::spawn(async move {
                                    for op in 0..OPS_PER_TASK {
                                        let is_read = {
                                            let mut r = rand::rng();
                                            r.random::<f32>() < 0.7
                                        };
                                        if is_read {
                                            let key_index = {
                                                let mut r = rand::rng();
                                                r.random::<u32>() as usize
                                                    % 1000
                                            };
                                            let k = generate_key(key_index);
                                            let _ =
                                                store.get(&k).await.unwrap();
                                        } else {
                                            let k = generate_key(
                                                80_000 + t * 1000 + op,
                                            );
                                            let v = generate_value(100, op);
                                            store.put(&k, v).await.unwrap();
                                        }
                                    }
                                }));
                            }
                            for h in handles {
                                h.await.unwrap();
                            }
                        });
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_concurrent_operations);
criterion_main!(benches);
