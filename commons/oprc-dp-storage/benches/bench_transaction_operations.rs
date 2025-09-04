use criterion::{
    BatchSize, BenchmarkId, Criterion, Throughput, criterion_group,
    criterion_main,
};
use oprc_dp_storage::StorageBackend;
use tokio::runtime::Runtime;

#[path = "common/mod.rs"]
mod common;
use common::*;

const SMALL_VALUE_SIZE: usize = 100;

fn bench_transaction_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("transaction_operations");

    for &batch_size in &[10usize, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size as u64));

        // Memory
        let memory = create_memory_storage();
        group.bench_with_input(
            BenchmarkId::new("transaction_memory", batch_size),
            &batch_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        (0..size)
                            .map(|i| {
                                (
                                    generate_key(i),
                                    generate_value(SMALL_VALUE_SIZE, i),
                                )
                            })
                            .collect::<Vec<_>>()
                    },
                    |data| {
                        rt.block_on(async {
                            let _tx = memory.begin_transaction().unwrap();
                            for (k, v) in data {
                                memory.put(&k, v).await.unwrap();
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
                BenchmarkId::new("transaction_fjall", batch_size),
                &batch_size,
                |b, &size| {
                    b.iter_batched(
                        || {
                            (0..size)
                                .map(|i| {
                                    (
                                        generate_key(i),
                                        generate_value(SMALL_VALUE_SIZE, i),
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                        |data| {
                            rt.block_on(async {
                                let _tx = fjall.begin_transaction().unwrap();
                                for (k, v) in data {
                                    fjall.put(&k, v).await.unwrap();
                                }
                            })
                        },
                        BatchSize::SmallInput,
                    );
                },
            );

            let (fjall_tx, _t) = create_fjall_tx_storage();
            group.bench_with_input(
                BenchmarkId::new("transaction_fjall_tx", batch_size),
                &batch_size,
                |b, &size| {
                    b.iter_batched(
                        || {
                            (0..size)
                                .map(|i| {
                                    (
                                        generate_key(i),
                                        generate_value(SMALL_VALUE_SIZE, i),
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                        |data| {
                            rt.block_on(async {
                                let _tx = fjall_tx.begin_transaction().unwrap();
                                for (k, v) in data {
                                    fjall_tx.put(&k, v).await.unwrap();
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
                BenchmarkId::new("transaction_redb", batch_size),
                &batch_size,
                |b, &size| {
                    b.iter_batched(
                        || {
                            (0..size)
                                .map(|i| {
                                    (
                                        generate_key(i),
                                        generate_value(SMALL_VALUE_SIZE, i),
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                        |data| {
                            rt.block_on(async {
                                let _tx = redb.begin_transaction().unwrap();
                                for (k, v) in data {
                                    redb.put(&k, v).await.unwrap();
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
                BenchmarkId::new("transaction_skiplist", batch_size),
                &batch_size,
                |b, &size| {
                    b.iter_batched(
                        || {
                            (0..size)
                                .map(|i| {
                                    (
                                        generate_key(i),
                                        generate_value(SMALL_VALUE_SIZE, i),
                                    )
                                })
                                .collect::<Vec<_>>()
                        },
                        |data| {
                            rt.block_on(async {
                                let _tx = skiplist.begin_transaction().unwrap();
                                for (k, v) in data {
                                    skiplist.put(&k, v).await.unwrap();
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

criterion_group!(benches, bench_transaction_operations);
criterion_main!(benches);
