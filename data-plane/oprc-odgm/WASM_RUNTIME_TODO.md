# WASM Runtime Integration TODO

Tracks the remaining work to complete full WebAssembly runtime support in
OaaS-RS / ODGM.  See `WASM_RUNTIME_DESIGN.md` for the architectural overview.

---

## Phase 1 – Local Invocation Infrastructure ✅ Done

- [x] Define `LocalFnOffloader` in `commons/oprc-invoke/src/local.rs`
  - [x] `register_fn(fn_id, closure)` for stateless functions
  - [x] `register_obj_fn(fn_id, closure)` for object methods
  - [x] Fallback from `invoke_obj` to stateless handler
  - [x] Unit tests (echo, transform, error, fallback, obj precedence)
- [x] Implement `InvocationExecutor` for `LocalFnOffloader`
- [x] Add `local_offloader` field to `ObjectUnifiedShard`
- [x] Add `with_local_offloader()` builder method
- [x] Update `trait_impl.rs` to check local offloader before remote gRPC
- [x] Integration tests in `data-plane/oprc-odgm/tests/local_invocation_test.rs`
- [x] Design doc (`WASM_RUNTIME_DESIGN.md`) and TODO (`WASM_RUNTIME_TODO.md`)

---

## Phase 2 – WASM Engine Integration 🔲 Pending

- [ ] Add `wasmtime` dependency to workspace `Cargo.toml` (feature-gated: `wasm`)
- [ ] Create `commons/oprc-invoke/src/wasm.rs`
  - [ ] `WasmFnOffloader` struct holding `wasmtime::Engine` + module registry
  - [ ] `load_module(fn_id, wasm_bytes)` to pre-compile and cache modules
  - [ ] `load_module_from_file(fn_id, path)` convenience constructor
  - [ ] Implement `InvocationExecutor` for `WasmFnOffloader`
  - [ ] Define host/WASM ABI (memory allocation helpers, `invoke` export)
  - [ ] Unit tests with a minimal echo WASM module
- [ ] Extend `FuncInvokeRoute` proto with `runtime_type` enum
  (`Grpc` | `WasmLocal` | future variants)
- [ ] Update shard builder: detect `wasm://` URL → create `WasmFnOffloader`

---

## Phase 3 – WASM Module Lifecycle 🔲 Pending

- [ ] Module hot-reload: watch `.wasm` file for changes, re-compile on update
- [ ] Module versioning: allow multiple versions in-flight
- [ ] Resource limits: configure `wasmtime::ResourceLimiter` (memory cap, fuel)
- [ ] WASI integration: decide which capabilities are allowed (stdout-only default)

---

## Phase 4 – E2E Testing 🔲 Pending

- [ ] Compile a sample Rust function to WASM (`tests/fixtures/echo.wasm`)
- [ ] Add an `oprc-cli` smoke test deploying a WASM-backed class and invoking it
- [ ] Extend `tests/system_e2e` with a WASM invocation scenario
- [ ] Benchmark: local WASM invocation latency vs remote gRPC invocation

---

## Phase 5 – Production Hardening 🔲 Pending

- [ ] WASM binary signing / integrity verification before loading
- [ ] Metrics: `wasm_invocations_total`, `wasm_invocation_duration_seconds`
- [ ] Structured `wasmtime::Error` → `OffloadError` mapping
- [ ] Update `ODGM_ARCHITECTURE.md` with WASM execution path
- [ ] CRM `ClassRuntime` spec: field for WASM module URL / OCI image reference
