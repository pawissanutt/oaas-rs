use criterion::{
    criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion,
    Throughput,
};
use oprc_dp_storage::{
    backends::{fjall::FjallStorage, memory::MemoryStorage},
    snapshot::SnapshotCapableStorage,
    traits::StorageBackend,
    StorageConfig, StorageValue,
};

#[cfg(feature = "skiplist")]
use oprc_dp_storage::backends::skiplist::SkipListStorage;
use rand::{rng, Rng};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Benchmark configuration
const SMALL_DATASET_SIZE: usize = 100; // Reduced for quick testing
const MEDIUM_DATASET_SIZE: usize = 1_000; // Reduced for quick testing

const SMALL_VALUE_SIZE: usize = 100;
const MEDIUM_VALUE_SIZE: usize = 1_024;
const LARGE_VALUE_SIZE: usize = 10_240;

/// Generate test data
fn generate_key(i: usize) -> StorageValue {
    let key_data = format!("benchmark_key_{:08}", i).into_bytes();
    StorageValue::from(key_data)
}

fn generate_value(size: usize, seed: usize) -> StorageValue {
    let pattern = format!("value_data_{:08}_", seed);
    let mut data = Vec::with_capacity(size);

    while data.len() < size {
        let remaining = size - data.len();
        let chunk = if remaining >= pattern.len() {
            pattern.as_bytes()
        } else {
            &pattern.as_bytes()[..remaining]
        };
        data.extend_from_slice(chunk);
    }

    StorageValue::from(data)
}

/// Create storage backends for benchmarking
fn create_memory_storage() -> Arc<MemoryStorage> {
    let config = StorageConfig::memory();
    Arc::new(
        MemoryStorage::new(config).expect("Failed to create memory storage"),
    )
}

fn create_fjall_storage() -> (Arc<FjallStorage>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config =
        StorageConfig::fjall(temp_dir.path().to_string_lossy().to_string());
    let storage = Arc::new(
        FjallStorage::new(config).expect("Failed to create fjall storage"),
    );
    (storage, temp_dir)
}

#[cfg(feature = "skiplist")]
fn create_skiplist_storage() -> Arc<SkipListStorage> {
    let config = StorageConfig::memory();
    Arc::new(
        SkipListStorage::new(config)
            .expect("Failed to create skiplist storage"),
    )
}

/// Benchmark single operations across backends
fn bench_single_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("single_operations");

    for &value_size in &[SMALL_VALUE_SIZE, MEDIUM_VALUE_SIZE, LARGE_VALUE_SIZE]
    {
        group.throughput(Throughput::Bytes(value_size as u64));

        // Memory storage benchmarks
        let memory = create_memory_storage();

        group.bench_with_input(
            BenchmarkId::new("put_memory", value_size),
            &value_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let mut rng = rng();
                        let key = generate_key(rng.random::<u32>() as usize);
                        let value =
                            generate_value(size, rng.random::<u32>() as usize);
                        (key, value)
                    },
                    |(key, value)| {
                        rt.block_on(async {
                            memory.put(&key, value).await.unwrap();
                        })
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // Pre-populate for GET benchmark
        rt.block_on(async {
            for i in 0..1000 {
                let key = generate_key(i);
                let value = generate_value(value_size, i);
                memory.put(&key, value).await.unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::new("get_memory", value_size),
            &value_size,
            |b, &_size| {
                b.iter_batched(
                    || {
                        let mut rng = rng();
                        generate_key(rng.random_range(0..1000))
                    },
                    |key| {
                        rt.block_on(async {
                            let _ = memory.get(&key).await.unwrap();
                        })
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // Fjall storage benchmarks
        let (fjall, _temp_dir) = create_fjall_storage();

        group.bench_with_input(
            BenchmarkId::new("put_fjall", value_size),
            &value_size,
            |b, &size| {
                b.iter_batched(
                    || {
                        let mut rng = rng();
                        let key = generate_key(rng.random::<u32>() as usize);
                        let value =
                            generate_value(size, rng.random::<u32>() as usize);
                        (key, value)
                    },
                    |(key, value)| {
                        rt.block_on(async {
                            fjall.put(&key, value).await.unwrap();
                        })
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // Pre-populate for GET benchmark
        rt.block_on(async {
            for i in 0..1000 {
                let key = generate_key(i);
                let value = generate_value(value_size, i);
                fjall.put(&key, value).await.unwrap();
            }
        });

        group.bench_with_input(
            BenchmarkId::new("get_fjall", value_size),
            &value_size,
            |b, &_size| {
                b.iter_batched(
                    || generate_key(rng().random_range(0..1000)),
                    |key| {
                        rt.block_on(async {
                            let _ = fjall.get(&key).await.unwrap();
                        })
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // SkipList storage benchmarks
        #[cfg(feature = "skiplist")]
        {
            let skiplist = create_skiplist_storage();

            group.bench_with_input(
                BenchmarkId::new("put_skiplist", value_size),
                &value_size,
                |b, &size| {
                    b.iter_batched(
                        || {
                            let mut rng = rng();
                            let key =
                                generate_key(rng.random::<u32>() as usize);
                            let value = generate_value(
                                size,
                                rng.random::<u32>() as usize,
                            );
                            (key, value)
                        },
                        |(key, value)| {
                            rt.block_on(async {
                                skiplist.put(&key, value).await.unwrap();
                            })
                        },
                        BatchSize::SmallInput,
                    );
                },
            );

            // Pre-populate for GET benchmark
            rt.block_on(async {
                for i in 0..1000 {
                    let key = generate_key(i);
                    let value = generate_value(value_size, i);
                    skiplist.put(&key, value).await.unwrap();
                }
            });

            group.bench_with_input(
                BenchmarkId::new("get_skiplist", value_size),
                &value_size,
                |b, &_size| {
                    b.iter_batched(
                        || {
                            generate_key(
                                rand::rng().random::<u32>() as usize % 1000,
                            )
                        },
                        |key| {
                            rt.block_on(async {
                                let _ = skiplist.get(&key).await.unwrap();
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

/// Benchmark batch operations
fn bench_batch_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("batch_operations");

    for &dataset_size in &[SMALL_DATASET_SIZE, MEDIUM_DATASET_SIZE] {
        group.throughput(Throughput::Elements(dataset_size as u64));

        // Memory storage batch PUT
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
                            for (key, value) in data {
                                memory.put(&key, value).await.unwrap();
                            }
                        })
                    },
                    BatchSize::LargeInput,
                );
            },
        );

        // Fjall storage batch PUT
        let (fjall, _temp_dir) = create_fjall_storage();
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
                            for (key, value) in data {
                                fjall.put(&key, value).await.unwrap();
                            }
                        })
                    },
                    BatchSize::LargeInput,
                );
            },
        );

        // SkipList storage batch PUT
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
                                for (key, value) in data {
                                    skiplist.put(&key, value).await.unwrap();
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

/// Benchmark scan operations
fn bench_scan_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("scan_operations");

    // Setup memory storage with data
    let memory = create_memory_storage();
    rt.block_on(async {
        for i in 0..MEDIUM_DATASET_SIZE {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            memory.put(&key, value).await.unwrap();
        }
    });

    // Setup fjall storage with data
    let (fjall, _temp_dir) = create_fjall_storage();
    rt.block_on(async {
        for i in 0..MEDIUM_DATASET_SIZE {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            fjall.put(&key, value).await.unwrap();
        }
    });

    // Setup skiplist storage with data
    #[cfg(feature = "skiplist")]
    let skiplist = {
        let skiplist = create_skiplist_storage();
        rt.block_on(async {
            for i in 0..MEDIUM_DATASET_SIZE {
                let key = generate_key(i);
                let value = generate_value(SMALL_VALUE_SIZE, i);
                skiplist.put(&key, value).await.unwrap();
            }
        });
        skiplist
    };

    // Benchmark prefix scan for memory
    group.bench_function("scan_prefix_memory", |b| {
        b.iter(|| {
            rt.block_on(async {
                let prefix = b"benchmark_key_0001";
                let _ = memory.scan(prefix).await.unwrap();
            })
        });
    });

    // Benchmark prefix scan for fjall
    group.bench_function("scan_prefix_fjall", |b| {
        b.iter(|| {
            rt.block_on(async {
                let prefix = b"benchmark_key_0001";
                let _ = fjall.scan(prefix).await.unwrap();
            })
        });
    });

    // Benchmark range scan for memory
    group.bench_function("scan_range_memory", |b| {
        b.iter(|| {
            rt.block_on(async {
                let start_key = generate_key(1000).into_vec();
                let end_key = generate_key(5000).into_vec();
                let _ = memory.scan_range(start_key..end_key).await.unwrap();
            })
        });
    });

    // Benchmark range scan for fjall
    group.bench_function("scan_range_fjall", |b| {
        b.iter(|| {
            rt.block_on(async {
                let start_key = generate_key(1000).into_vec();
                let end_key = generate_key(5000).into_vec();
                let _ = fjall.scan_range(start_key..end_key).await.unwrap();
            })
        });
    });

    // SkipList scan benchmarks
    #[cfg(feature = "skiplist")]
    {
        // Benchmark prefix scan for skiplist
        group.bench_function("scan_prefix_skiplist", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let prefix = b"benchmark_key_0001";
                    let _ = skiplist.scan(prefix).await.unwrap();
                })
            });
        });

        // Benchmark range scan for skiplist
        group.bench_function("scan_range_skiplist", |b| {
            b.iter(|| {
                rt.block_on(async {
                    let start_key = generate_key(1000).into_vec();
                    let end_key = generate_key(5000).into_vec();
                    let _ =
                        skiplist.scan_range(start_key..end_key).await.unwrap();
                })
            });
        });
    }

    group.finish();
}

/// Benchmark snapshot operations
fn bench_snapshot_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("snapshot_operations");

    // Setup memory storage with data
    let memory = create_memory_storage();
    rt.block_on(async {
        for i in 0..SMALL_DATASET_SIZE {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            memory.put(&key, value).await.unwrap();
        }
    });

    // Setup fjall storage with data
    let (fjall, _temp_dir) = create_fjall_storage();
    rt.block_on(async {
        for i in 0..SMALL_DATASET_SIZE {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            fjall.put(&key, value).await.unwrap();
        }
    });

    // Setup skiplist storage with data
    #[cfg(feature = "skiplist")]
    let skiplist = {
        let skiplist = create_skiplist_storage();
        rt.block_on(async {
            for i in 0..SMALL_DATASET_SIZE {
                let key = generate_key(i);
                let value = generate_value(SMALL_VALUE_SIZE, i);
                skiplist.put(&key, value).await.unwrap();
            }
        });
        skiplist
    };

    // Benchmark snapshot creation for memory
    group.bench_function("snapshot_create_memory", |b| {
        b.iter(|| {
            rt.block_on(async {
                memory.create_snapshot().await.unwrap();
            })
        });
    });

    // Benchmark snapshot creation for fjall
    group.bench_function("snapshot_create_fjall", |b| {
        b.iter(|| {
            rt.block_on(async {
                fjall.create_snapshot().await.unwrap();
            })
        });
    });

    // SkipList snapshot benchmarks
    #[cfg(feature = "skiplist")]
    {
        // Benchmark snapshot creation for skiplist
        group.bench_function("snapshot_create_skiplist", |b| {
            b.iter(|| {
                rt.block_on(async {
                    skiplist.create_snapshot().await.unwrap();
                })
            });
        });
    }

    group.finish();
}

/// Benchmark transaction operations
fn bench_transaction_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("transaction_operations");

    for &batch_size in &[10, 100, 1000] {
        group.throughput(Throughput::Elements(batch_size as u64));

        // Memory storage transactions
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
                            let _tx = memory.begin_transaction().await.unwrap();
                            for (key, value) in data {
                                memory.put(&key, value).await.unwrap();
                            }
                        })
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // Fjall storage transactions
        let (fjall, _temp_dir) = create_fjall_storage();
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
                            let _tx = fjall.begin_transaction().await.unwrap();
                            for (key, value) in data {
                                fjall.put(&key, value).await.unwrap();
                            }
                        })
                    },
                    BatchSize::SmallInput,
                );
            },
        );

        // SkipList storage transactions
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
                                let _tx =
                                    skiplist.begin_transaction().await.unwrap();
                                for (key, value) in data {
                                    skiplist.put(&key, value).await.unwrap();
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

/// Benchmark mixed workloads
fn bench_mixed_workload(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("mixed_workload");

    // Setup memory storage with initial data
    let memory = create_memory_storage();
    rt.block_on(async {
        for i in 0..1000 {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            memory.put(&key, value).await.unwrap();
        }
    });

    // Setup fjall storage with initial data
    let (fjall, _temp_dir) = create_fjall_storage();
    rt.block_on(async {
        for i in 0..1000 {
            let key = generate_key(i);
            let value = generate_value(SMALL_VALUE_SIZE, i);
            fjall.put(&key, value).await.unwrap();
        }
    });

    // Memory mixed workload (70% reads, 30% writes)
    group.bench_function("mixed_read_heavy_memory", |b| {
        b.iter_batched(
            || {
                let mut operations = Vec::new();

                // 70% reads
                for _ in 0..70 {
                    operations.push((
                        "read",
                        generate_key(
                            rand::rng().random::<u32>() as usize % 1000,
                        ),
                    ));
                }

                // 30% writes
                for i in 0..30 {
                    operations.push(("write", generate_key(1000 + i)));
                }

                operations
            },
            |operations| {
                rt.block_on(async {
                    for (op_type, key) in operations {
                        match op_type {
                            "read" => {
                                let _ = memory.get(&key).await.unwrap();
                            }
                            "write" => {
                                let value = generate_value(SMALL_VALUE_SIZE, 0);
                                memory.put(&key, value).await.unwrap();
                            }
                            _ => unreachable!(),
                        }
                    }
                })
            },
            BatchSize::SmallInput,
        );
    });

    // Fjall mixed workload (70% reads, 30% writes)
    group.bench_function("mixed_read_heavy_fjall", |b| {
        b.iter_batched(
            || {
                let mut operations = Vec::new();

                // 70% reads
                for _ in 0..70 {
                    operations.push((
                        "read",
                        generate_key(
                            rand::rng().random::<u32>() as usize % 1000,
                        ),
                    ));
                }

                // 30% writes
                for i in 0..30 {
                    operations.push(("write", generate_key(1000 + i)));
                }

                operations
            },
            |operations| {
                rt.block_on(async {
                    for (op_type, key) in operations {
                        match op_type {
                            "read" => {
                                let _ = fjall.get(&key).await.unwrap();
                            }
                            "write" => {
                                let value = generate_value(SMALL_VALUE_SIZE, 0);
                                fjall.put(&key, value).await.unwrap();
                            }
                            _ => unreachable!(),
                        }
                    }
                })
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_single_operations,
    bench_batch_operations,
    bench_scan_operations,
    bench_snapshot_operations,
    bench_transaction_operations,
    bench_mixed_workload,
);

criterion_main!(benches);
