use bincode::serde::{decode_from_slice, encode_to_vec};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use oprc_dp_storage::{
    AnyStorage, StorageBackend, StorageBackendType, StorageConfig,
    storage_value::StorageValue,
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tokio::runtime::Runtime;

// Minimal benchmark-local replica of ObjectVal / ObjectEntry to avoid cyclic deps.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum BenchValType {
    Byte,
    CrdtMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchObjectVal {
    data: Vec<u8>,
    vtype: BenchValType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BenchObjectEntry {
    last_updated: u64,
    value: BTreeMap<u32, BenchObjectVal>,
    str_value: BTreeMap<String, BenchObjectVal>,
    // Keep an optional event payload placeholder to approximate shape
    event: Option<Vec<u8>>,
}

impl BenchObjectEntry {
    fn new() -> Self {
        Self {
            last_updated: 0,
            value: BTreeMap::new(),
            str_value: BTreeMap::new(),
            event: None,
        }
    }
}
// rand & tokio already imported above

// Key layout examples (simplified for benchmark):
// Blob key: b"obj:<id>:blob"
// Entry key: b"obj:<id>:e:<u32_be>"

const ENTRY_COUNTS: &[u32] = &[1, 8, 32, 128, 512];
const VALUE_SIZES: &[usize] = &[16, 128, 1024, 4096];
const OBJECT_ID: &str = "benchmark-obj-1"; // single object focus

fn make_value(rng: &mut StdRng, size: usize) -> Vec<u8> {
    let mut v = vec![0u8; size];
    rng.fill(&mut v[..]);
    v
}

fn blob_key() -> Vec<u8> {
    format!("obj:{}:blob", OBJECT_ID).into_bytes()
}
fn entry_key(idx: u32) -> Vec<u8> {
    format!("obj:{}:e:{:08x}", OBJECT_ID, idx).into_bytes()
}

pub fn bench_blob_vs_entry_scan_storage(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("storage_blob_vs_entry_scan");

    // Enumerate backends we will test via AnyStorage (feature gated)
    let mut backends: Vec<(StorageBackendType, AnyStorage)> = Vec::new();

    // Memory (always)
    backends.push((
        StorageBackendType::Memory,
        AnyStorage::open(StorageConfig::memory()).expect("open memory"),
    ));

    // SkipList (if feature)
    #[cfg(feature = "skiplist")]
    {
        backends.push((
            StorageBackendType::SkipList,
            AnyStorage::open(StorageConfig::skiplist()).expect("open skiplist"),
        ));
    }

    // Fjall (if feature) requires temp directory
    #[cfg(feature = "fjall")]
    {
        use tempfile::TempDir;
        let dir = TempDir::new().expect("tempdir fjall");
        let cfg =
            StorageConfig::fjall(dir.path().to_string_lossy().to_string());
        backends.push((
            StorageBackendType::Fjall,
            AnyStorage::open(cfg).expect("open fjall"),
        ));
        // Note: TempDir kept alive by moving into leak to extend for benchmark lifetime
        Box::leak(Box::new(dir));
    }

    for (btype, backend) in backends.into_iter() {
        for &entries in ENTRY_COUNTS {
            for &val_sz in VALUE_SIZES {
                // Clear namespace: delete all obj: keys
                rt.block_on(async {
                    if let Ok(all) = backend.scan(b"obj:").await {
                        for (k, _) in all {
                            let _ = backend.delete(k.as_slice()).await;
                        }
                    }
                });
                let mut rng = StdRng::seed_from_u64(
                    0xBEEF00 + ((entries as u64) << 16) + val_sz as u64,
                );
                // Build BenchObjectEntry and serialize with bincode
                let mut bench_entry = BenchObjectEntry::new();
                for i in 0..entries {
                    let val = make_value(&mut rng, val_sz);
                    bench_entry.value.insert(
                        i,
                        BenchObjectVal {
                            data: val.clone(),
                            vtype: BenchValType::Byte,
                        },
                    );
                }
                // Add a tiny event payload (simulating trigger meta) for shape realism
                bench_entry.event = Some(vec![1, 2, 3, 4]);
                let blob_buf =
                    encode_to_vec(&bench_entry, bincode::config::standard())
                        .expect("bincode encode");
                let bkey = blob_key();
                rt.block_on(async {
                    backend
                        .put(&bkey, StorageValue::from(blob_buf))
                        .await
                        .unwrap();
                });
                // Persist per-entry values individually (raw value bytes only)
                for (i, v) in bench_entry.value.iter() {
                    let key = entry_key(*i);
                    let data = v.data.clone();
                    rt.block_on(async {
                        backend
                            .put(&key, StorageValue::from(data))
                            .await
                            .unwrap();
                    });
                }

                // Bench blob read
                group.bench_with_input(
                    BenchmarkId::new(
                        format!("{}:blob_read", btype),
                        format!("e{}_v{}", entries, val_sz),
                    ),
                    &(entries, val_sz),
                    |b, _| {
                        b.iter(|| {
                            rt.block_on(async {
                                let raw =
                                    backend.get(&bkey).await.unwrap().unwrap();
                                let (decoded, _): (BenchObjectEntry, _) =
                                    decode_from_slice(
                                        raw.as_slice(),
                                        bincode::config::standard(),
                                    )
                                    .expect("bincode decode");
                                let mut total = 0usize;
                                for v in decoded.value.values() {
                                    total += v.data.len();
                                }
                                std::hint::black_box(total);
                            })
                        })
                    },
                );

                // Bench entry scan
                let prefix = format!("obj:{}:e:", OBJECT_ID).into_bytes();
                group.bench_with_input(
                    BenchmarkId::new(
                        format!("{}:entry_scan_reconstruct_bincode", btype),
                        format!("e{}_v{}", entries, val_sz),
                    ),
                    &(entries, val_sz),
                    |b, _| {
                        b.iter(|| {
                            rt.block_on(async {
                                let list = backend.scan(&prefix).await.unwrap();
                                // Reconstruct BenchObjectEntry
                                let mut rebuilt = BenchObjectEntry::new();
                                for (k, v) in list {
                                    if let Ok(key_str) =
                                        std::str::from_utf8(k.as_slice())
                                    {
                                        if let Some(hex_idx) =
                                            key_str.rsplit(':').next()
                                        {
                                            if let Ok(idx) =
                                                u32::from_str_radix(hex_idx, 16)
                                            {
                                                rebuilt.value.insert(
                                                    idx,
                                                    BenchObjectVal {
                                                        data: v
                                                            .as_slice()
                                                            .to_vec(),
                                                        vtype:
                                                            BenchValType::Byte,
                                                    },
                                                );
                                            }
                                        }
                                    }
                                }
                                let encoded = encode_to_vec(
                                    &rebuilt,
                                    bincode::config::standard(),
                                )
                                .expect("encode rebuilt");
                                std::hint::black_box(encoded.len());
                            })
                        })
                    },
                );
            }
        }
    }

    group.finish();
}

criterion_group!(benches, bench_blob_vs_entry_scan_storage);
criterion_main!(benches);
