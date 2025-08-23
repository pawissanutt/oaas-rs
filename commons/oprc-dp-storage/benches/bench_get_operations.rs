use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use oprc_dp_storage::StorageBackend;
use tokio::runtime::Runtime;

#[path = "common/mod.rs"]
mod common;
use common::*;

const SMALL_VALUE_SIZE: usize = 100;
const MEDIUM_VALUE_SIZE: usize = 1_024;
const LARGE_VALUE_SIZE: usize = 10_240;

fn bench_get_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("get_operations");

    for &preload_size in
        &[SMALL_VALUE_SIZE, MEDIUM_VALUE_SIZE, LARGE_VALUE_SIZE]
    {
        group.throughput(Throughput::Bytes(preload_size as u64));

        // Memory
        let memory = create_memory_storage();
        rt.block_on(async {
            for i in 0..1000 {
                let key = generate_key(i);
                let value = generate_value(preload_size, i);
                memory.put(&key, value).await.unwrap();
            }
        });
        bench_get_iter!(
            group,
            rt,
            BenchmarkId::new("get_memory", preload_size),
            memory
        );

        // Fjall
        #[cfg(feature = "fjall")]
        {
            let (fjall, _t) = create_fjall_storage();
            rt.block_on(async {
                for i in 0..1000 {
                    let key = generate_key(i);
                    let value = generate_value(preload_size, i);
                    fjall.put(&key, value).await.unwrap();
                }
            });
            bench_get_iter!(
                group,
                rt,
                BenchmarkId::new("get_fjall", preload_size),
                fjall
            );
        }

        // Redb
        #[cfg(feature = "redb")]
        {
            let (redb, _t) = create_redb_storage();
            rt.block_on(async {
                for i in 0..1000 {
                    let key = generate_key(i);
                    let value = generate_value(preload_size, i);
                    redb.put(&key, value).await.unwrap();
                }
            });
            bench_get_iter!(
                group,
                rt,
                BenchmarkId::new("get_redb", preload_size),
                redb
            );
        }

        // SkipList
        #[cfg(feature = "skiplist")]
        {
            let skiplist = create_skiplist_storage();
            rt.block_on(async {
                for i in 0..1000 {
                    let key = generate_key(i);
                    let value = generate_value(preload_size, i);
                    skiplist.put(&key, value).await.unwrap();
                }
            });
            bench_get_iter!(
                group,
                rt,
                BenchmarkId::new("get_skiplist", preload_size),
                skiplist
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_get_operations);
criterion_main!(benches);
