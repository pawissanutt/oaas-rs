// Shared helpers and macros for benchmarks
#[cfg(feature = "fjall")]
use oprc_dp_storage::backends::fjall::FjallStorage;
#[cfg(feature = "fjall")]
use oprc_dp_storage::backends::fjall::FjallTxStorage;
use oprc_dp_storage::{
    StorageConfig, StorageValue, backends::memory::MemoryStorage,
};

#[cfg(feature = "redb")]
use oprc_dp_storage::backends::redb::RedbStorage;
#[cfg(feature = "skiplist")]
use oprc_dp_storage::backends::skiplist::SkipListStorage;

// no direct rand/criterion imports here; macros call fully-qualified paths
use std::sync::Arc;
#[cfg(any(feature = "fjall", feature = "redb"))]
use tempfile::TempDir;

// Data generation -----------------------------------------------------------
pub fn generate_key(i: usize) -> StorageValue {
    let key_data = format!("benchmark_key_{:08}", i).into_bytes();
    StorageValue::from(key_data)
}

pub fn generate_value(size: usize, seed: usize) -> StorageValue {
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

// Storage creators ----------------------------------------------------------
pub fn create_memory_storage() -> Arc<MemoryStorage> {
    let config = StorageConfig::memory();
    Arc::new(
        MemoryStorage::new(config).expect("Failed to create memory storage"),
    )
}

#[cfg(feature = "fjall")]
pub fn create_fjall_storage() -> (Arc<FjallStorage>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config =
        StorageConfig::fjall(temp_dir.path().to_string_lossy().to_string());
    let storage = Arc::new(
        FjallStorage::new(config).expect("Failed to create fjall storage"),
    );
    (storage, temp_dir)
}

#[cfg(feature = "fjall")]
#[allow(dead_code)]
pub fn create_fjall_tx_storage() -> (Arc<FjallTxStorage>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let config =
        StorageConfig::fjall(temp_dir.path().to_string_lossy().to_string());
    let storage = Arc::new(
        FjallTxStorage::new(config).expect("Failed to create fjall tx storage"),
    );
    (storage, temp_dir)
}

#[cfg(feature = "redb")]
#[allow(dead_code)]
pub fn create_redb_storage() -> (Arc<RedbStorage>, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("bench.redb");
    let config = StorageConfig::redb(db_path.to_string_lossy().to_string());
    let storage = Arc::new(
        RedbStorage::new(config).expect("Failed to create redb storage"),
    );
    (storage, temp_dir)
}

#[cfg(feature = "skiplist")]
pub fn create_skiplist_storage() -> Arc<SkipListStorage> {
    let config = StorageConfig::memory();
    Arc::new(
        SkipListStorage::new(config)
            .expect("Failed to create skiplist storage"),
    )
}

// Macros --------------------------------------------------------------------
#[macro_export]
macro_rules! bench_put_iter {
    ($group:expr, $rt:expr, $id:expr, $store:expr, $value_size:expr) => {{
        $group.bench_with_input($id, &$value_size, |b, &size| {
            b.iter_batched(
                || {
                    let mut rng = rand::rng();
                    let key = generate_key(
                        rand::Rng::random::<u32>(&mut rng) as usize
                    );
                    let value = generate_value(
                        size,
                        rand::Rng::random::<u32>(&mut rng) as usize,
                    );
                    (key, value)
                },
                |(key, value)| {
                    $rt.block_on(async {
                        $store.put(&key, value).await.unwrap();
                    })
                },
                ::criterion::BatchSize::SmallInput,
            );
        });
    }};
}

#[macro_export]
macro_rules! bench_get_iter {
    ($group:expr, $rt:expr, $id:expr, $store:expr) => {{
        $group.bench_with_input($id, &0usize, |b, &_size| {
            b.iter_batched(
                || {
                    generate_key(rand::Rng::random_range(
                        &mut rand::rng(),
                        0..1000,
                    ))
                },
                |key| {
                    $rt.block_on(async {
                        let _ = $store.get(&key).await.unwrap();
                    })
                },
                ::criterion::BatchSize::SmallInput,
            );
        });
    }};
}
