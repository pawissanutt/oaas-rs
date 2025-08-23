use criterion::{
    criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion,
    Throughput,
};
use oprc_dp_storage::StorageBackend;
use tokio::runtime::Runtime;

#[path = "common/mod.rs"]
mod common;
use common::*;

const SMALL_DATASET_SIZE: usize = 100;
const SMALL_VALUE_SIZE: usize = 100;

fn bench_batch_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("batch_operations");

    for &dataset_size in &[SMALL_DATASET_SIZE, 1_000] {
        group.throughput(Throughput::Elements(dataset_size as u64));

        // Memory
        let memory = create_memory_storage();
        group.bench_with_input(
            BenchmarkId::new("batch_put_memory", dataset_size),
            &dataset_size,
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
                            for (k, v) in data {
                                memory.put(&k, v).await.unwrap();
                            }
                        })
                    },
                    BatchSize::LargeInput,
                );
            },
        );

        // Fjall
        #[cfg(feature = "fjall")]
        {
            let (fjall, _t) = create_fjall_storage();
            group.bench_with_input(
                BenchmarkId::new("batch_put_fjall", dataset_size),
                &dataset_size,
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
                                for (k, v) in data {
                                    fjall.put(&k, v).await.unwrap();
                                }
                            })
                        },
                        BatchSize::LargeInput,
                    );
                },
            );

            let (fjall_tx, _t) = create_fjall_tx_storage();
            group.bench_with_input(
                BenchmarkId::new("batch_put_fjall_tx", dataset_size),
                &dataset_size,
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
                                for (k, v) in data {
                                    fjall_tx.put(&k, v).await.unwrap();
                                }
                            })
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }

        // Redb
        #[cfg(feature = "redb")]
        {
            let (redb, _t) = create_redb_storage();
            group.bench_with_input(
                BenchmarkId::new("batch_put_redb", dataset_size),
                &dataset_size,
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
                                for (k, v) in data {
                                    redb.put(&k, v).await.unwrap();
                                }
                            })
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }

        // SkipList
        #[cfg(feature = "skiplist")]
        {
            let skiplist = create_skiplist_storage();
            group.bench_with_input(
                BenchmarkId::new("batch_put_skiplist", dataset_size),
                &dataset_size,
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
                                for (k, v) in data {
                                    skiplist.put(&k, v).await.unwrap();
                                }
                            })
                        },
                        BatchSize::LargeInput,
                    );
                },
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_batch_operations);
criterion_main!(benches);
