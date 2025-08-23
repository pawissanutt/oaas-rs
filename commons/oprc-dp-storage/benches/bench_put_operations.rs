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

fn bench_put_operations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("put_operations");

    for &value_size in &[SMALL_VALUE_SIZE, MEDIUM_VALUE_SIZE, LARGE_VALUE_SIZE]
    {
        group.throughput(Throughput::Bytes(value_size as u64));

        // Memory
        let memory = create_memory_storage();
        bench_put_iter!(
            group,
            rt,
            BenchmarkId::new("put_memory", value_size),
            memory,
            value_size
        );

        // Fjall
        #[cfg(feature = "fjall")]
        {
            let (fjall, _t) = create_fjall_storage();
            bench_put_iter!(
                group,
                rt,
                BenchmarkId::new("put_fjall", value_size),
                fjall,
                value_size
            );

            let (fjall_tx, _t) = create_fjall_tx_storage();
            bench_put_iter!(
                group,
                rt,
                BenchmarkId::new("put_fjall_tx", value_size),
                fjall_tx,
                value_size
            );
        }

        // Redb
        #[cfg(feature = "redb")]
        {
            let (redb, _t) = create_redb_storage();
            bench_put_iter!(
                group,
                rt,
                BenchmarkId::new("put_redb", value_size),
                redb,
                value_size
            );
        }

        // SkipList
        #[cfg(feature = "skiplist")]
        {
            let skiplist = create_skiplist_storage();
            bench_put_iter!(
                group,
                rt,
                BenchmarkId::new("put_skiplist", value_size),
                skiplist,
                value_size
            );
        }
    }

    group.finish();
}

criterion_group!(benches, bench_put_operations);
criterion_main!(benches);
