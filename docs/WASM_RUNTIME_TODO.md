# WASM Runtime — Implementation TODO (TDD)

> Each phase: write tests first → implement to pass → refactor.

## Phase 1: Domain Model Changes (`oprc-models`) ✅

- [x] **Test**: `FunctionType::Wasm` serializes to `"WASM"` and deserializes back
- [x] **Test**: `ProvisionConfig` with `wasm_module_url` round-trips through JSON
- [x] **Test**: `ProvisionConfig::default()` has `wasm_module_url: None`
- [x] **Impl**: Add `Wasm` variant to `FunctionType` enum
- [x] **Impl**: Add `wasm_module_url: Option<String>` to `ProvisionConfig`

## Phase 2: CRD & Route Model (`oprc-crm`) ✅

- [x] **Test**: `FunctionRoute` with `wasm_module_url` serializes/deserializes correctly
- [x] **Test**: `predicted_function_routes()` generates `wasm://` URL when function has `wasm_module_url`
- [x] **Test**: `render_with()` skips Deployment/Service for WASM functions
- [x] **Impl**: Add `wasm_module_url: Option<String>` to `FunctionRoute` in CRD
- [x] **Impl**: Update `predicted_function_routes()` for wasm scheme
- [x] **Impl**: Update `render_with()` to skip WASM function deployments
- [x] **Impl**: Add `wasm_module_url` to gRPC proto `ProvisionConfig` + PM builder

## Phase 3: New Crate Scaffold (`oprc-wasm`) ✅

- [x] **Scaffold**: Create `data-plane/oprc-wasm` crate with `Cargo.toml`
- [x] **Scaffold**: Add to workspace `Cargo.toml` (member + wasmtime dependency)
- [x] **Scaffold**: Write `wit/oaas.wit` — types, data-access imports, guest-function exports (invoke-fn, invoke-obj)
- [x] **Scaffold**: `bindgen!` in `src/lib.rs` → verify it compiles

## Phase 4: Module Store (`oprc-wasm::store`) ✅

- [x] **Test**: Load a `.wasm` component from bytes → module cached by fn_id
- [x] **Test**: `get()` returns the cached module; missing fn_id returns None
- [x] **Test**: `remove()` drops the module
- [x] **Test**: Unsupported URL scheme returns error
- [x] **Impl**: `WasmModuleStore` — `load()`, `load_from_bytes()`, `get()`, `remove()`

## Phase 5: Host Functions (`oprc-wasm::host`) ✅

- [x] **Impl**: Define `OdgmDataOps` trait (async, object/entry CRUD + invoke)
- [x] **Impl**: `WasmHostState` struct (data ops + invocation context)
- [x] **Impl**: `DataOpsError` enum

## Phase 6: WASM Executor (`oprc-wasm::executor`) ✅

- [x] **Impl**: `WasmInvocationExecutor` skeleton with wasmtime Linker

## Phase 7: ODGM Integration (`oprc-odgm`) ✅

- [x] **Impl**: Add `oprc-wasm` optional dependency (feature `wasm`)
- [x] **Impl**: Update `Features` struct with `wasm_runtime` field
- [x] **Impl**: `CapabilitiesProvider` reports `wasm_runtime: cfg!(feature = "wasm")`

## Phase 8: End-to-End Integration Test

- [ ] **Test**: Build a Rust guest WASM component that reads object → transforms → writes entry
- [ ] **Test**: Full pipeline: configure ODGM collection with wasm route → invoke → verify object state changed
- [ ] **Test**: Hot-reload: update module URL → re-invoke → verify new behavior

## Phase 9: Guest SDK & Examples

- [ ] Create `examples/wasm-echo` — minimal echo function (stateless)
- [ ] Create `examples/wasm-transform` — object method that reads + writes entries
- [ ] Document guest development workflow in `docs/WASM_GUEST_GUIDE.md`
