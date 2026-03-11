//! Benchmarks for the WASM runtime on ODGM.
//!
//! Measures invocation latency (isolated from storage via [`MockDataOps`]) and
//! module compilation time using:
//! - **Rust guest** (`wasm-guest-echo`): lightweight, ~2 MB
//! - **TypeScript guest** (`wasm-guest-ts-counter`): realistic SDK-based,
//!   compiled via ComponentizeJS (~11 MB, embeds SpiderMonkey)
//!
//! **Pre-requisites** — build both guests before running:
//! ```sh
//! # Rust guest
//! cargo build -p wasm-guest-echo --target wasm32-wasip2 --release
//! # TypeScript guest (requires Node.js + npm install in tools/oprc-compiler)
//! cd tools/oprc-compiler && npx tsx scripts/build-bench-guest.ts
//! ```
//! Or use the justfile shortcut:
//! ```sh
//! just build-wasm-guest
//! ```

use std::hint::black_box;
use std::path::Path;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use oprc_wasm::executor::{OopContext, WasmInvocationExecutor};
use oprc_wasm::host::OdgmDataOps;
use oprc_wasm::mock_ops::MockDataOps;
use oprc_wasm::store::WasmModuleStore;
use tokio::runtime::Runtime;
use wasmtime::Engine;

// ─── Helpers ──────────────────────────────────────────

/// Resolve path to the pre-compiled guest WASM component.
fn wasm_guest_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join(format!("target/wasm32-wasip2/release/{name}.wasm"))
}

/// Resolve path to the TS guest compiled by ComponentizeJS.
fn ts_guest_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .join("target/wasm-ts-guest/counter.wasm")
}

fn read_guest_bytes_sync() -> Vec<u8> {
    let path = wasm_guest_path("wasm_guest_echo");
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "Guest component not found at {path:?}. \
             Run: cargo build -p wasm-guest-echo --target wasm32-wasip2 --release"
        )
    })
}

fn read_ts_guest_bytes_sync() -> Option<Vec<u8>> {
    let path = ts_guest_path();
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            eprintln!(
                "WARNING: TS guest not found at {path:?}. \
                 Skipping TS benchmarks. Build with: \
                 cd tools/oprc-compiler && npx tsx scripts/build-bench-guest.ts"
            );
            None
        }
    }
}

fn wasm_engine() -> Engine {
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);
    Engine::new(&config).expect("failed to create wasmtime engine")
}

fn oop_ctx() -> OopContext {
    OopContext {
        remote_proxy: None,
        shard_cls_id: "bench-cls".into(),
        shard_partition_id: 0,
    }
}

/// Shared fixture: engine + executor with guest loaded under "echo" and
/// "transform" fn_ids.
struct WasmBenchFixture {
    executor: Arc<WasmInvocationExecutor>,
}

impl WasmBenchFixture {
    async fn new() -> Self {
        let engine = wasm_engine();
        let store = Arc::new(WasmModuleStore::new(engine));
        let wasm_bytes = read_guest_bytes_sync();

        store
            .load_from_bytes("echo", &wasm_bytes)
            .await
            .expect("load echo module");
        store
            .load_from_bytes("transform", &wasm_bytes)
            .await
            .expect("load transform module");

        let executor = Arc::new(
            WasmInvocationExecutor::new(store).expect("create executor"),
        );

        Self { executor }
    }
}

/// Fixture for the TypeScript guest (Counter service via oaas-sdk-ts).
struct TsBenchFixture {
    executor: Arc<WasmInvocationExecutor>,
}

impl TsBenchFixture {
    async fn new(wasm_bytes: &[u8]) -> Self {
        let engine = wasm_engine();
        let store = Arc::new(WasmModuleStore::new(engine));

        // The TS counter exposes: increment, getCount, reset, echo, failOnPurpose
        // We load the same module under several fn_ids so the executor can
        // dispatch by fn_id (which becomes the function_name in on-invoke).
        for fn_id in ["increment", "echo", "getCount", "reset"] {
            store
                .load_from_bytes(fn_id, wasm_bytes)
                .await
                .unwrap_or_else(|e| panic!("load ts module as {fn_id}: {e}"));
        }

        let executor = Arc::new(
            WasmInvocationExecutor::new(store).expect("create ts executor"),
        );

        Self { executor }
    }
}

// ─── Benchmark: Module compilation ────────────────────

fn bench_module_compilation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let rust_bytes = read_guest_bytes_sync();
    let ts_bytes = read_ts_guest_bytes_sync();

    let mut group = c.benchmark_group("wasm_module_compilation");

    group.bench_function("rust_guest", |b| {
        let bytes = rust_bytes.clone();
        b.to_async(&rt).iter(|| {
            let bytes = bytes.clone();
            async move {
                let engine = wasm_engine();
                let store = WasmModuleStore::new(engine);
                store
                    .load_from_bytes("bench", &bytes)
                    .await
                    .expect("compile module");
                black_box(store.len().await);
            }
        });
    });

    if let Some(ref ts) = ts_bytes {
        let ts = ts.clone();
        group.bench_function("ts_guest", |b| {
            let bytes = ts.clone();
            b.to_async(&rt).iter(|| {
                let bytes = bytes.clone();
                async move {
                    let engine = wasm_engine();
                    let store = WasmModuleStore::new(engine);
                    store
                        .load_from_bytes("bench", &bytes)
                        .await
                        .expect("compile ts module");
                    black_box(store.len().await);
                }
            });
        });
    }

    group.finish();
}

// ─── Benchmark: invoke_fn (stateless echo) ────────────

const PAYLOAD_SIZES: &[usize] =
    &[0, 1024, 4096, 16384, 128 * 1024, 512 * 1024, 1024 * 1024];

fn make_payload(size: usize) -> Option<Vec<u8>> {
    if size == 0 {
        None
    } else {
        Some(vec![0xABu8; size])
    }
}

fn bench_invoke_fn_echo(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = rt.block_on(WasmBenchFixture::new());
    let ctx = oop_ctx();

    let mut group = c.benchmark_group("wasm_invoke_fn_echo");

    for &size in PAYLOAD_SIZES {
        let payload = make_payload(size);

        group.bench_with_input(
            BenchmarkId::new("payload_bytes", size),
            &size,
            |b, _| {
                let executor = fixture.executor.clone();
                let payload = payload.clone();
                let ctx = ctx.clone();
                b.to_async(&rt).iter(|| {
                    let executor = executor.clone();
                    let payload = payload.clone();
                    let ctx = ctx.clone();
                    async move {
                        let data_ops = Box::new(MockDataOps::default());
                        let result = executor
                            .invoke_fn(
                                "echo",
                                "bench-cls",
                                0,
                                payload,
                                data_ops,
                                Some(&ctx),
                            )
                            .await
                            .expect("invoke_fn echo");
                        black_box(result);
                    }
                });
            },
        );
    }

    group.finish();
}

// ─── Benchmark: invoke_obj (stateful transform) ──────

fn bench_invoke_obj_transform(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let fixture = rt.block_on(WasmBenchFixture::new());
    let ctx = oop_ctx();

    let mut group = c.benchmark_group("wasm_invoke_obj_transform");

    for &size in PAYLOAD_SIZES {
        let initial_data = if size == 0 {
            b"seed".to_vec()
        } else {
            vec![0xCDu8; size]
        };

        group.bench_with_input(
            BenchmarkId::new("payload_bytes", size),
            &size,
            |b, _| {
                let executor = fixture.executor.clone();
                let initial_data = initial_data.clone();
                let ctx = ctx.clone();
                b.to_async(&rt).iter(|| {
                    let executor = executor.clone();
                    let initial_data = initial_data.clone();
                    let ctx = ctx.clone();
                    async move {
                        // Fresh MockDataOps per iteration so the transform
                        // handler always finds the "_raw" key to read/modify.
                        let mock_ops = MockDataOps::default();
                        mock_ops
                            .set_value(
                                "bench-cls",
                                0,
                                "obj-1",
                                "_raw",
                                initial_data,
                            )
                            .await
                            .expect("seed _raw");

                        let data_ops = Box::new(mock_ops);
                        let result = executor
                            .invoke_obj(
                                "transform",
                                "bench-cls",
                                0,
                                "obj-1",
                                None,
                                data_ops,
                                Some(&ctx),
                            )
                            .await
                            .expect("invoke_obj transform");
                        black_box(result);
                    }
                });
            },
        );
    }

    group.finish();
}

// ─── Wire up ──────────────────────────────────────────

// ── TS guest: stateless echo ──────────────────────────

fn bench_ts_invoke_fn_echo(c: &mut Criterion) {
    let ts_bytes = match read_ts_guest_bytes_sync() {
        Some(b) => b,
        None => return,
    };
    let rt = Runtime::new().unwrap();
    let fixture = rt.block_on(TsBenchFixture::new(&ts_bytes));
    let ctx = oop_ctx();

    let mut group = c.benchmark_group("ts_invoke_fn_echo");

    for &size in PAYLOAD_SIZES {
        let payload = make_payload(size);

        group.bench_with_input(
            BenchmarkId::new("payload_bytes", size),
            &size,
            |b, _| {
                let executor = fixture.executor.clone();
                let payload = payload.clone();
                let ctx = ctx.clone();
                b.to_async(&rt).iter(|| {
                    let executor = executor.clone();
                    let payload = payload.clone();
                    let ctx = ctx.clone();
                    async move {
                        let data_ops = Box::new(MockDataOps::default());
                        let result = executor
                            .invoke_fn(
                                "echo",
                                "bench-cls",
                                0,
                                payload,
                                data_ops,
                                Some(&ctx),
                            )
                            .await
                            .expect("ts invoke_fn echo");
                        black_box(result);
                    }
                });
            },
        );
    }

    group.finish();
}

// ── TS guest: stateful increment ──────────────────────

fn bench_ts_invoke_obj_increment(c: &mut Criterion) {
    let ts_bytes = match read_ts_guest_bytes_sync() {
        Some(b) => b,
        None => return,
    };
    let rt = Runtime::new().unwrap();
    let fixture = rt.block_on(TsBenchFixture::new(&ts_bytes));
    let ctx = oop_ctx();

    let mut group = c.benchmark_group("ts_invoke_obj_increment");

    for &size in PAYLOAD_SIZES {
        // The increment method expects a JSON number (amount).
        // For size=0 we use a small default, for others we pad with whitespace
        // to approximate the payload size while remaining valid JSON.
        let payload = if size == 0 {
            Some(b"1".to_vec())
        } else {
            // Build a JSON payload of roughly `size` bytes:
            // {"amount":1, ...padding...}
            let base = b"{\"amount\":1}";
            if size <= base.len() {
                Some(base.to_vec())
            } else {
                // Pad with leading whitespace (valid JSON)
                let mut buf = vec![b' '; size - base.len()];
                buf.extend_from_slice(base);
                Some(buf)
            }
        };

        group.bench_with_input(
            BenchmarkId::new("payload_bytes", size),
            &size,
            |b, _| {
                let executor = fixture.executor.clone();
                let payload = payload.clone();
                let ctx = ctx.clone();
                b.to_async(&rt).iter(|| {
                    let executor = executor.clone();
                    let payload = payload.clone();
                    let ctx = ctx.clone();
                    async move {
                        // Pre-seed state fields as JSON so the SDK
                        // loadFields step succeeds.
                        let mock_ops = MockDataOps::default();
                        mock_ops
                            .set_value(
                                "bench-cls",
                                0,
                                "obj-1",
                                "count",
                                b"0".to_vec(),
                            )
                            .await
                            .expect("seed count");
                        mock_ops
                            .set_value(
                                "bench-cls",
                                0,
                                "obj-1",
                                "history",
                                b"[]".to_vec(),
                            )
                            .await
                            .expect("seed history");

                        let data_ops = Box::new(mock_ops);
                        let result = executor
                            .invoke_obj(
                                "increment",
                                "bench-cls",
                                0,
                                "obj-1",
                                payload,
                                data_ops,
                                Some(&ctx),
                            )
                            .await
                            .expect("ts invoke_obj increment");
                        black_box(result);
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(compilation, bench_module_compilation);
criterion_group!(invoke_fn_echo, bench_invoke_fn_echo);
criterion_group!(invoke_obj_transform, bench_invoke_obj_transform);
criterion_group!(ts_echo, bench_ts_invoke_fn_echo);
criterion_group!(ts_increment, bench_ts_invoke_obj_increment);

criterion_main!(
    compilation,
    invoke_fn_echo,
    invoke_obj_transform,
    ts_echo,
    ts_increment
);
