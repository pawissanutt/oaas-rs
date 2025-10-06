use std::collections::HashMap;
use std::hint::black_box;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use oprc_dp_storage::backends::fjall::FjallStorage;
use oprc_dp_storage::backends::memory::MemoryStorage;
use oprc_dp_storage::{ApplicationDataStorage, StorageConfig};
use oprc_odgm::events::EventManagerImpl;
use oprc_odgm::granular_trait::{EntryListOptions, EntryStore};
use oprc_odgm::replication::no_replication::NoReplication;
use oprc_odgm::shard::unified::ObjectUnifiedShard;
use oprc_odgm::shard::unified::traits::ShardMetadata;
use oprc_odgm::shard::{ObjectVal, UnifiedShardConfig};
use tempfile::tempdir;
use tokio::runtime::Runtime;

const OBJECT_ID: &str = "bench::granular-object";
const ENTRY_COUNT: usize = 256;
const BATCH_SIZE: usize = 50;
const ENABLE_STRING_IDS: bool = true;
const MAX_STRING_ID_LEN: usize = 160;
const ENABLE_GRANULAR_STORAGE: bool = true;
const GRANULAR_PREFETCH_LIMIT: usize = 256;

struct BenchFixture {
    memory: Arc<
        ObjectUnifiedShard<
            MemoryStorage,
            NoReplication<MemoryStorage>,
            EventManagerImpl<MemoryStorage>,
        >,
    >,
    fjall: Arc<
        ObjectUnifiedShard<
            FjallStorage,
            NoReplication<FjallStorage>,
            EventManagerImpl<FjallStorage>,
        >,
    >,
    _fjall_dir: tempfile::TempDir,
    keys: Vec<String>,
}

impl BenchFixture {
    async fn new() -> Self {
        let memory_storage =
            MemoryStorage::new(StorageConfig::memory()).unwrap();
        let fjall_dir = tempdir().unwrap();
        let fjall_path = fjall_dir.path().join("granular-bench");
        let fjall_storage = FjallStorage::new(StorageConfig::fjall(
            fjall_path.to_string_lossy().to_string(),
        ))
        .unwrap();

        let memory = Arc::new(
            setup_shard(memory_storage.clone())
                .await
                .expect("memory shard setup"),
        );
        let fjall = Arc::new(
            setup_shard(fjall_storage.clone())
                .await
                .expect("fjall shard setup"),
        );

        let mut keys = Vec::with_capacity(ENTRY_COUNT);
        for idx in 0..ENTRY_COUNT {
            let key = format!("key-{idx:04}");
            let value = ObjectVal {
                data: format!("value-{idx:04}").into_bytes(),
                r#type: oprc_grpc::ValType::Byte,
            };
            memory
                .set_entry(OBJECT_ID, &key, value.clone())
                .await
                .expect("set memory entry");
            fjall
                .set_entry(OBJECT_ID, &key, value)
                .await
                .expect("set fjall entry");
            keys.push(key);
        }

        Self {
            memory,
            fjall,
            _fjall_dir: fjall_dir,
            keys,
        }
    }
}

async fn setup_shard<S>(
    storage: S,
) -> Result<
    ObjectUnifiedShard<S, NoReplication<S>, EventManagerImpl<S>>,
    oprc_odgm::shard::unified::ShardError,
>
where
    S: ApplicationDataStorage + Clone + Send + Sync + 'static,
{
    let metadata = create_metadata();
    let replication = NoReplication::new(storage.clone());
    let shard_config = UnifiedShardConfig {
        enable_string_ids: ENABLE_STRING_IDS,
        max_string_id_len: MAX_STRING_ID_LEN,
        granular_prefetch_limit: GRANULAR_PREFETCH_LIMIT,
    };
    let shard = ObjectUnifiedShard::new_minimal(
        metadata,
        storage,
        replication,
        shard_config,
    )
    .await?;
    shard.initialize().await?;
    Ok(shard)
}

fn create_metadata() -> ShardMetadata {
    ShardMetadata {
        id: 42,
        collection: "bench_collection".to_string(),
        partition_id: 0,
        owner: None,
        primary: None,
        replica: vec![],
        replica_owner: vec![],
        shard_type: "memory".to_string(),
        options: Default::default(),
        invocations: Default::default(),
        storage_config: None,
        replication_config: None,
        consistency_config: None,
    }
}

fn bench_entry_get(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = rt.block_on(BenchFixture::new());

    let key = fixture.keys[ENTRY_COUNT / 2].clone();

    c.bench_with_input(
        BenchmarkId::new("entry_get", "memory"),
        &key,
        |b, k| {
            let shard = fixture.memory.clone();
            b.to_async(&rt).iter(|| async {
                let value =
                    shard.get_entry(OBJECT_ID, k).await.expect("get entry");
                black_box(value);
            });
        },
    );

    c.bench_with_input(BenchmarkId::new("entry_get", "fjall"), &key, |b, k| {
        let shard = fixture.fjall.clone();
        b.to_async(&rt).iter(|| async {
            let value = shard.get_entry(OBJECT_ID, k).await.expect("get entry");
            black_box(value);
        });
    });
}

fn bench_list_entries(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = rt.block_on(BenchFixture::new());

    let options = EntryListOptions::with_limit(100);

    c.bench_function("list_entries_memory", |b| {
        let shard = fixture.memory.clone();
        b.to_async(&rt).iter(|| async {
            let result = shard
                .list_entries(OBJECT_ID, options.clone())
                .await
                .expect("list entries");
            black_box(result.entries.len());
        });
    });

    c.bench_function("list_entries_fjall", |b| {
        let shard = fixture.fjall.clone();
        b.to_async(&rt).iter(|| async {
            let result = shard
                .list_entries(OBJECT_ID, options.clone())
                .await
                .expect("list entries");
            black_box(result.entries.len());
        });
    });
}

fn bench_batch_set(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = rt.block_on(BenchFixture::new());

    let batch: HashMap<String, ObjectVal> = (0..BATCH_SIZE)
        .map(|i| {
            (
                format!("batch-{i:03}"),
                ObjectVal {
                    data: vec![i as u8; 64],
                    r#type: oprc_grpc::ValType::Byte,
                },
            )
        })
        .collect();

    c.bench_function("batch_set_memory", |b| {
        let shard = fixture.memory.clone();
        let batch_values = batch.clone();
        b.to_async(&rt).iter(|| async {
            let version = shard
                .batch_set_entries(OBJECT_ID, batch_values.clone(), None)
                .await
                .expect("batch set");
            black_box(version);
        });
    });

    c.bench_function("batch_set_fjall", |b| {
        let shard = fixture.fjall.clone();
        let batch_values = batch.clone();
        b.to_async(&rt).iter(|| async {
            let version = shard
                .batch_set_entries(OBJECT_ID, batch_values.clone(), None)
                .await
                .expect("batch set");
            black_box(version);
        });
    });
}

fn benchmarks(c: &mut Criterion) {
    bench_entry_get(c);
    bench_list_entries(c);
    bench_batch_set(c);
}

criterion_group!(granular_entry_store, benchmarks);
criterion_main!(granular_entry_store);
