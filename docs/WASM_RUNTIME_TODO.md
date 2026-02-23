# WASM Runtime — Implementation TODO (TDD)

> Each phase: write tests first → implement to pass → refactor.

## Phase 1: Domain Model Changes (`oprc-models`)

- [ ] **Test**: `FunctionType::Wasm` serializes to `"WASM"` and deserializes back
- [ ] **Test**: `ProvisionConfig` with `wasm_module_url` round-trips through JSON
- [ ] **Test**: `ProvisionConfig::default()` has `wasm_module_url: None`
- [ ] **Impl**: Add `Wasm` variant to `FunctionType` enum
- [ ] **Impl**: Add `wasm_module_url: Option<String>` to `ProvisionConfig`

## Phase 2: CRD & Route Model (`oprc-crm`)

- [ ] **Test**: `FunctionRoute` with `wasm_module_url` serializes/deserializes correctly
- [ ] **Test**: `predicted_function_routes()` generates `wasm://` URL when function has `wasm_module_url`
- [ ] **Test**: `render_with()` skips Deployment/Service for WASM functions
- [ ] **Test**: `render_with()` still renders ODGM Deployment with wasm route in collection config
- [ ] **Impl**: Add `wasm_module_url: Option<String>` to `FunctionRoute` in CRD
- [ ] **Impl**: Update `predicted_function_routes()` for wasm scheme
- [ ] **Impl**: Update `render_with()` to skip WASM function deployments

## Phase 3: New Crate Scaffold (`oprc-wasm`)

- [ ] **Scaffold**: Create `data-plane/oprc-wasm` crate with `Cargo.toml`
- [ ] **Scaffold**: Add to workspace `Cargo.toml` (member + dependency)
- [ ] **Scaffold**: Write `wit/oaas.wit` — types, data-access imports, guest-function exports (invoke-fn, invoke-obj)
- [ ] **Scaffold**: `bindgen!` in `src/lib.rs` → verify it compiles

## Phase 4: Module Store (`oprc-wasm::store`)

- [ ] **Test**: Load a `.wasm` component from bytes → module cached by fn_id
- [ ] **Test**: `get()` returns the cached module; missing fn_id returns error
- [ ] **Test**: `remove()` drops the module
- [ ] **Test**: Load from HTTP URL (mock server via `wiremock`)
- [ ] **Impl**: `WasmModuleStore` — `load()`, `get()`, `remove()`

## Phase 5: Host Functions (`oprc-wasm::host`)

- [ ] **Test**: `OdgmDataOps` mock — `get_object` returns expected data
- [ ] **Test**: `OdgmDataOps` mock — `set_value` stores correctly
- [ ] **Test**: Host trait impl delegates to `OdgmDataOps` trait
- [ ] **Impl**: Define `OdgmDataOps` trait
- [ ] **Impl**: Implement generated `data_access::Host` for `WasmHostState`

## Phase 6: WASM Executor (`oprc-wasm::executor`)

- [ ] **Test**: Build a minimal Rust guest (echo function) → compile to `wasm32-wasip2`
- [ ] **Test**: `WasmInvocationExecutor::invoke_fn()` — stateless invocation returns expected response
- [ ] **Test**: `WasmInvocationExecutor::invoke_obj()` — object method receives `object_id`, calls host `get-object`
- [ ] **Test**: Error handling — guest returns `app-error` status
- [ ] **Test**: Fuel exhaustion — infinite loop guest is terminated
- [ ] **Impl**: `WasmInvocationExecutor` implementing `InvocationExecutor` trait
- [ ] **Impl**: Request/response conversion (proto ↔ WIT types)
- [ ] **Impl**: Fuel metering + timeout configuration

## Phase 7: ODGM Integration (`oprc-odgm`)

- [ ] **Test**: `InvocationOffloader` routes `wasm://` to WASM executor, `http://` to gRPC
- [ ] **Test**: `ShardDataOpsAdapter` bridges shard `get_object` → `OdgmDataOps`
- [ ] **Test**: `ShardDataOpsAdapter` bridges shard `set_value` → `OdgmDataOps`
- [ ] **Test**: Events emitted after WASM invocation (FunctionComplete / FunctionError)
- [ ] **Test**: Capabilities report `wasm_runtime: true` when feature enabled
- [ ] **Impl**: Add `oprc-wasm` optional dependency (feature `wasm`)
- [ ] **Impl**: `ShardDataOpsAdapter` wrapping `ObjectUnifiedShard`
- [ ] **Impl**: Update `InvocationOffloader` dispatcher logic
- [ ] **Impl**: Update `Features` struct

## Phase 8: End-to-End Integration Test

- [ ] **Test**: Build a Rust guest WASM component that reads object → transforms → writes entry
- [ ] **Test**: Full pipeline: configure ODGM collection with wasm route → invoke → verify object state changed
- [ ] **Test**: Hot-reload: update module URL → re-invoke → verify new behavior

## Phase 9: Guest SDK & Examples

- [ ] Create `examples/wasm-echo` — minimal echo function (stateless)
- [ ] Create `examples/wasm-transform` — object method that reads + writes entries
- [ ] Document guest development workflow in `docs/WASM_GUEST_GUIDE.md`
