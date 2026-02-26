# Scripting Runtime — Implementation TODO

> Phases for implementing the OOP scripting layer described in [SCRIPTING_RUNTIME_DESIGN.md](SCRIPTING_RUNTIME_DESIGN.md).
> Dependencies between phases are noted; independent phases can run in parallel.

> **Compatibility note:** The previous `oaas-function` world (procedural `invoke-fn`/`invoke-obj`) was a proof-of-concept.
> Backward compatibility with it is **not required**. The `oaas-object` world is the target going forward;
> the legacy world may be removed once all guests migrate.

## Phase 1: OOP WIT Interface (`oprc-wasm`) ✅

> Prerequisite: None. Builds on existing `data-plane/oprc-wasm/wit/oaas.wit`.

- [x] Design the `object-context` interface in WIT
  - [x] Add `object-ref` record: `{ cls: string, partition-id: u32, object-id: string }`
  - [x] Add `field-entry` record: `{ key: string, value: list<u8> }`
  - [x] Define `resource object-proxy` with methods:
    - [x] `ref() → object-ref`
    - [x] `get(key: string) → result<option<list<u8>>, odgm-error>` — single field read
    - [x] `get-many(keys: list<string>) → result<list<field-entry>, odgm-error>` — batch field read
    - [x] `set(key: string, value: list<u8>) → result<_, odgm-error>` — single field write
    - [x] `set-many(entries: list<field-entry>) → result<_, odgm-error>` — batch field write
    - [x] `delete(key: string) → result<_, odgm-error>`
    - [x] `get-all() → result<obj-data, odgm-error>` — full object read
    - [x] `set-all(data: obj-data) → result<_, odgm-error>` — full object write
    - [x] `invoke(fn-name: string, payload: option<list<u8>>) → result<option<list<u8>>, odgm-error>`
  - [x] Add context functions:
    - [x] `object(ref: object-ref) → result<object-proxy, odgm-error>` — get proxy to any object
    - [x] `object-by-str(ref-str: string) → result<object-proxy, odgm-error>` — parse `"cls/partition/id"`
    - [x] `log(level: log-level, message: string)` with `log-level` enum (debug, info, warn, error)
- [x] Design the `guest-object` interface (exports)
  - [x] `on-invoke(self: object-proxy, function-name: string, payload: option<list<u8>>, headers: list<key-value>) → invocation-response`
    - Note: `self` proxy is created by the host from invocation context and passed as first parameter
- [x] Define new WIT world `oaas-object` (imports `object-context`, exports `guest-object`)
- [x] Verify WIT compiles: `wasm-tools component wit data-plane/oprc-wasm/wit/`
- [x] Add `bindgen!` for the new world in `oprc-wasm/src/lib.rs` (alongside existing `oaas-function` bindings)
- [x] Verify crate compiles: `cargo check -p oprc-wasm -q`

## Phase 2: Host Implementation for `object-proxy` resource (`oprc-wasm`) ✅

> Prerequisite: Phase 1.

- [x] Implement `object-proxy` as a wasmtime resource
  - [x] Host-side struct `ObjectProxyState` holding `object-ref` + local `Arc<dyn OdgmDataOps>` + remote RPC client
  - [x] Locality check: compare proxy's `object-ref` against current shard's class/partition
  - [x] Register as wasmtime resource type in the Linker (via bindgen `with:` clause)
- [x] Implement `object-proxy` resource methods on `ObjectProxyState`
  - [x] `ref` → return stored `object-ref`
  - [x] `get` → local: `OdgmDataOps::get_value`; remote: `ObjectProxy` Zenoh RPC
  - [x] `get-many` → batch calls to `get_value` (local) or batch RPC (remote)
  - [x] `set` → local: `OdgmDataOps::set_value`; remote: `ObjectProxy` Zenoh RPC
  - [x] `set-many` → batch calls to `set_value` (local) or batch RPC (remote)
  - [x] `delete` → local: `OdgmDataOps::delete_value`; remote: `ObjectProxy` Zenoh RPC
  - [x] `get-all` → local: `OdgmDataOps::get_object`; remote: `ObjectProxy::get_obj`
  - [x] `set-all` → local: `OdgmDataOps::set_object`; remote: `ObjectProxy::set_obj`
  - [x] `invoke` → local: `OdgmDataOps::invoke_obj`; remote: `ObjectProxy::invoke_object_fn`
    - Added `invoke_obj(cls_id, partition_id, object_id, fn_id, payload)` to `OdgmDataOps` trait.
  - [x] Verify field keys route to shard granular entries (`get_entry_granular(id, key)`) — not through `_raw` blob convention
- [x] Implement `object-context` host functions
  - [x] `object(ref)` → create `ObjectProxyState` with given ref, determine local/remote, return resource handle
  - [x] `object-by-str(ref-str)` → parse `"cls/partition/id"` into `object-ref`, validate no `/` in cls/id, create proxy
  - [x] `log` → route to `tracing::{debug,info,warn,error}!` macros with guest source label
- [x] Implement re-entrancy guard: track nesting depth in `ObjectWasmHostState`, enforce max depth (default: 4)
- [ ] Implement shared fuel: nested invocations consume from parent's fuel budget *(deferred — requires wasmtime fuel plumbing)*
- [x] Manage proxy lifecycle: store proxy handles in `ObjectWasmHostState` resource table (owned semantics)
- [x] Unit tests for each proxy method and context function (27 tests in `object_host.rs`)
  - [x] Test local proxy: get/set/invoke on same shard
  - [x] Test remote proxy: mock RPC client, verify routing for different partition
  - [x] Test re-entrancy depth limit
- [ ] Add configurable capacity limit to `WasmModuleStore` with LRU eviction for compiled modules *(deferred)*
- [x] Verify: `cargo check -p oprc-wasm -q`

## Phase 3: Executor Updates (`oprc-wasm`) ✅

> Prerequisite: Phase 2.

- [x] Add world detection in `WasmInvocationExecutor`
  - [x] Check which exports are present (`guest-function` vs `guest-object`) via `Component::component_type()` introspection
  - [x] Store world type per compiled module (enum: `WorldType::Legacy` / `WorldType::ObjectOriented`)
- [x] For `oaas-object` guests: map `invoke_fn` and `invoke_obj` calls to `on-invoke`
  - [x] Create self `object-proxy` from `OopContext` (`cls_id`, `partition_id`, `object_id`)
  - [x] Pass self proxy as first parameter to `on-invoke`
  - [x] Map `fn_id` → `function-name` parameter
- [x] For `oaas-function` guests: preserve existing behavior (unchanged; will be removed once migration is complete — see compatibility note)
- [x] Update `WasmExecutorAdapter` to handle both world types (carries `OopContext`)
- [x] Update `wasm_bridge.rs` to construct `OopContext` from shard metadata
- [ ] Integration test: load a guest targeting `oaas-object` world → invoke → verify host context works *(blocked on creating an `oaas-object` guest component — see Phase 9)*
- [x] Verify: `cargo test -p oprc-wasm -q` (63 unit tests passing)

## Phase 4: TypeScript SDK (`@oaas/sdk`)

> Prerequisite: Phase 1 (WIT definition). Can run in parallel with Phase 2-3.
> Modeled after the Python OaaS SDK patterns (decorated classes, auto-persisted fields, plain return values).

- [ ] Create directory: `sdk/typescript/` (or `tools/oaas-sdk-ts/`)
- [ ] Initialize npm package: `@oaas/sdk`
- [ ] Implement decorators
  - [ ] `@service(name: string, opts?: { package?: string })` — register class as OaaS service, store metadata
  - [ ] `@method(opts?: { stateless?: bool, timeout?: number })` — mark method as invocable function
  - [ ] `@getter(field?: string)` — read-only accessor (not exported as RPC)
  - [ ] `@setter(field?: string)` — write accessor (not exported as RPC)
- [ ] Implement `OaaSObject` abstract base class
  - [ ] `ref: ObjectRef` — own identity (set by SDK shim from invocation context)
  - [ ] `object(ref: ObjectRef | string): ObjectProxy` — get proxy to another object
  - [ ] `log(level: string, message: string)` — structured logging to host
  - [ ] Type-annotated fields → auto-persisted state (see state management shim below)
- [ ] Implement transparent state management shim
  - [ ] Before method call: `get-many` all declared fields → deserialize JSON → set on `this`
  - [ ] After method call: diff field values → `set-many` changed fields → serialize return value as response
  - [ ] On throw: wrap error as `app-error` (`OaaSError`) or `system-error` (other) response
  - [ ] Field discovery: instantiate class once at module init, `Object.keys(instance)` = field list
  - [ ] State diff: JSON-serialize each field before and after method call, compare strings. Correctly detects in-place mutations (e.g., `this.history.push(...)`).
  - [ ] Stateless methods (`@method({ stateless: true })`): skip load/save cycle entirely — no `get-many` before, no `set-many` after
- [ ] Implement `ObjectProxy` class (wraps WIT `object-proxy` resource, for cross-object access)
  - [ ] `get<T>(key: string): Promise<T | null>` — calls proxy `get`, deserializes JSON
  - [ ] `getMany<T>(...keys: string[]): Promise<Record<string, T>>` — batch read via `get-many`
  - [ ] `set<T>(key: string, value: T): Promise<void>` — serializes JSON, calls proxy `set`
  - [ ] `setMany(entries: Record<string, any>): Promise<void>` — batch write via `set-many`
  - [ ] `delete(key: string): Promise<void>` — calls proxy `delete`
  - [ ] `getAll(): Promise<ObjData>` — full object read
  - [ ] `invoke(fnName: string, payload?: any): Promise<any>` — invoke method on this object
  - [ ] `ref: ObjectRef` — the object's identity
  - [ ] `toString(): string` — returns `"cls/partition/objectId"`
- [ ] Implement `ObjectRef` class
  - [ ] Fields: `cls: string`, `partitionId: number`, `objectId: string`
  - [ ] `ObjectRef.from(cls, partition, id)` — constructor
  - [ ] `ObjectRef.parse(str)` — parse `"cls/partition/id"` string form
  - [ ] `toString()` — returns `"cls/partition/id"`
- [ ] Implement `OaaSError` class for user-thrown application errors
- [ ] Implement method dispatch shim (entry point that ComponentizeJS compiles)
  - [ ] Import user's default export class
  - [ ] Instantiate it
  - [ ] Wire `guest-object.on-invoke(self-proxy, fn-name, payload, headers)`:
    - [ ] Set `this.ref` from self-proxy identity
    - [ ] Load declared fields from self-proxy via `get-many` → set on instance
    - [ ] Look up method by `function-name` on instance
    - [ ] Deserialize payload → call method with deserialized arg
    - [ ] Diff fields → write changes via `set-many`
    - [ ] Serialize return value → wrap as `okay` response
  - [ ] Handle missing methods → return `invalid-request` response
  - [ ] Handle `OaaSError` → return `app-error` response
  - [ ] Handle unexpected errors → return `system-error` response
- [ ] Implement package metadata extraction from `@service` / `@method` decorators
  - [ ] Generate OPackage-compatible JSON/YAML at compile time
- [ ] Write TypeScript type declarations (`index.d.ts`) for Monaco IntelliSense
- [ ] Unit tests for serialization, state diff, method dispatch
- [ ] Write sample guests:
  - [ ] `sdk/typescript/examples/counter.ts` — stateful counter with `increment`
  - [ ] `sdk/typescript/examples/greeting.ts` — stateless function

## Phase 5: Compiler Service (`oprc-compiler`)

> Prerequisite: Phase 1 (WIT file), Phase 4 (SDK).

- [ ] Create directory: `tools/oprc-compiler/`
- [ ] Initialize Node.js project with dependencies
  - [ ] `@bytecodealliance/componentize-js`
  - [ ] `@bytecodealliance/jco`
  - [ ] `typescript` (compiler API)
  - [ ] `express` or `fastify` (HTTP server)
- [ ] Implement compilation pipeline
  - [ ] TypeScript → JavaScript transpilation (target ES2020, strip types)
  - [ ] Bundle user source + SDK shim into single JS file
  - [ ] Call `componentize(jsSource, witPath, worldName)` → WASM Component bytes
  - [ ] Return bytes or error messages
- [ ] Implement REST API
  - [ ] `POST /compile` → accepts `{ source, language }` → on success: returns `application/wasm` binary; on error: returns `{ success: false, errors: [] }` JSON
  - [ ] `GET /health` → returns `{ status: "ok" }`
- [ ] Error handling: TypeScript type errors, compilation failures, OOM protection
- [ ] Write Dockerfile (`tools/oprc-compiler/Dockerfile`)
  - [ ] Base: Node.js 20 slim
  - [ ] Pre-install dependencies
  - [ ] Copy WIT files from `data-plane/oprc-wasm/wit/`
- [ ] Manual test: compile a sample TypeScript function → verify output is valid WASM Component
- [ ] **Wasmtime compatibility test**: load compiled TS module in project's `wasmtime` version → confirm it instantiates and exports are correct
- [ ] Add to `docker-compose.dev.yml`

## Phase 6: PM Artifact Storage (`oprc-pm`)

> Prerequisite: None. Can run in parallel with Phases 1-5.

- [ ] Add artifact storage module to PM (`control-plane/oprc-pm/src/services/artifact.rs`)
  - [ ] Storage trait: `ArtifactStore` with `store(bytes) → id`, `get(id) → bytes`, `delete(id)`
  - [ ] Filesystem backend: write to `/data/wasm-modules/{content-hash}`, serve via HTTP
  - [ ] Content-hash addressing (SHA-256) for deduplication
- [ ] Add REST endpoint: `GET /api/v1/artifacts/{id}`
  - [ ] Serves raw WASM bytes with `application/wasm` content type
  - [ ] Streaming response for large modules
- [ ] Add artifact cleanup on deployment deletion
- [ ] Add PM config: `OPRC_ARTIFACT_DIR` (default `/data/wasm-modules/`)
- [ ] Add source code storage alongside packages
  - [ ] Store original TypeScript source in `OFunction` model (`source_code: Option<String>`) or separate source store
  - [ ] `GET /api/v1/scripts/{package}/{function}` → return stored source code for re-editing
- [ ] Unit tests for artifact store
- [ ] Verify: `cargo check -p oprc-pm -q`

## Phase 7: PM Script Endpoints (`oprc-pm`)

> Prerequisite: Phase 5 (compiler service running), Phase 6 (artifact store).

- [ ] Add compiler client module (`control-plane/oprc-pm/src/services/compiler.rs`)
  - [ ] HTTP client to compiler service: `POST /compile`
  - [ ] Timeout + retry configuration
  - [ ] Error propagation (compile errors → user-facing messages)
- [ ] Add PM config: `OPRC_COMPILER_URL` (default `http://oprc-compiler:3000`)
- [ ] Add REST endpoint: `POST /api/v1/scripts/compile`
  - [ ] Input: `{ source: string, language: string }`
  - [ ] Forwards to compiler service
  - [ ] Returns: `{ success: bool, errors?: string[] }` (validation only, no storage)
- [ ] Add REST endpoint: `POST /api/v1/scripts/deploy`
  - [ ] Input: `{ source, language, package_name, class_key, function_bindings, target_envs }`
  - [ ] Pipeline: compile → store artifact → **store source code** → create/update OPackage → create OClassDeployment
  - [ ] Returns: `{ deployment_key, artifact_url, status }`
- [ ] Add REST endpoint: `GET /api/v1/scripts/{package}/{function}`
  - [ ] Returns stored TypeScript source code for re-editing in frontend
- [ ] Integration test: submit source → verify package + deployment created with correct `wasm_module_url`
- [ ] Integration test: deploy → GET source → verify source matches original
- [ ] Verify: `cargo check -p oprc-pm -q`

## Phase 8: Frontend Script Editor (`oprc-gui`)

> Prerequisite: Phase 7 (PM script endpoints available).

### 8a: Monaco Editor Component
- [ ] Add Monaco CDN script tag to `Dioxus.toml` `<head>`
- [ ] Create `ScriptEditor` component (`frontend/oprc-gui/src/components/script_editor.rs`)
  - [ ] Initialize Monaco via `web_sys` / JS interop
  - [ ] TypeScript language mode
  - [ ] Register `@oaas/sdk` type definitions as extra lib for IntelliSense
  - [ ] Expose `get_value()` and `set_value()` methods to Rust via signals
  - [ ] Syntax error highlighting
- [ ] Verify: `cargo check -p oprc-gui`

### 8b: Scripts API Module
- [ ] Create `frontend/oprc-gui/src/api/scripts.rs`
  - [ ] `compile_script(source, language) → CompileResult`
  - [ ] `deploy_script(source, language, config) → DeployResult`
  - [ ] `list_scripts() → Vec<ScriptInfo>` (fetched from packages with WASM functions)
- [ ] Add to `frontend/oprc-gui/src/api/mod.rs`

### 8c: Scripts Page
- [ ] Create `ScriptsPage` component (`frontend/oprc-gui/src/components/pages/scripts.rs`)
  - [ ] Left sidebar: function list from packages, "New Function" button
  - [ ] Center: `ScriptEditor` component
  - [ ] Right panel: configuration form (class name, function bindings, target environments)
  - [ ] Bottom panel: console output (compile errors, deploy status)
  - [ ] "Compile" button → calls compile endpoint → shows errors in console
  - [ ] "Deploy" button → calls deploy endpoint → shows deployment status
- [ ] Add template pre-population for new functions
- [ ] Add route `/scripts` → `ScriptsPage` in `Route` enum (`frontend/oprc-gui/src/main.rs`)
- [ ] Add navbar entry for Scripts page
- [ ] Verify: `cargo check -p oprc-gui`

## Phase 9: Rust Guest Example Update (`oprc-wasm`)

> Prerequisite: Phase 3 (executor supports new world).
> The old `oaas-function` guest was a PoC — no backward-compatibility preservation needed.

- [ ] Update `tests/wasm-guest-echo/` to target the new `oaas-object` world
  - [ ] Implement `guest-object.on-invoke(self, fn-name, ...)` instead of `guest-function.invoke-fn`/`invoke-obj`
  - [ ] Use `self.get(key)` / `self.set(key, value)` proxy methods instead of explicit `data-access` calls
  - [ ] Demonstrate `object-ref` usage for cross-object access
- [ ] ~~Keep old test binary for backward-compatibility validation~~ *(not needed — previous version was PoC)*
- [ ] Verify: build with `cargo build -p wasm-guest-echo --target wasm32-wasip2`

## Phase 10: TypeScript Guest Example

> Prerequisite: Phase 5 (compiler service), Phase 4 (SDK).

- [ ] Create `tests/wasm-guest-ts-counter/`
  - [ ] TypeScript source: `Counter extends OaaSObject` with `increment` method
  - [ ] Compile via compiler service → produce WASM Component
  - [ ] Verify component loads in wasmtime
- [ ] Create `tests/system_e2e/fixtures/wasm-ts-package.yaml`
- [ ] Create `tests/system_e2e/fixtures/wasm-ts-deployment.yaml`

## Phase 11: End-to-End Integration

> Prerequisite: All previous phases.

- [ ] Extend system E2E (`tests/system_e2e/src/main.rs`) with TypeScript scenario
  - [ ] Submit TypeScript source to PM `/api/v1/scripts/deploy`
  - [ ] Wait for deployment to become ready
  - [ ] Invoke the function through the gateway
  - [ ] Validate the response
- [ ] Test hot-reload: update source → redeploy → verify new behavior
- [ ] Add compiler service to Kind cluster setup (`tools/kind-with-registry.sh` or Helm chart)
- [ ] Verify: `just system-e2e`

## Phase 12: Deployment Infrastructure

> Can run in parallel with other phases.

- [ ] Add compiler service Dockerfile to `build/`
- [ ] Add compiler service to `docker-compose.dev.yml`
- [ ] Add compiler service to Helm chart (`k8s/charts/`)
  - [ ] Deployment + Service for `oprc-compiler`
  - [ ] ConfigMap for WIT files
  - [ ] PM environment variable: `OPRC_COMPILER_URL`
- [ ] Add artifact volume mount to PM deployment (or S3 config for production)
- [ ] Update `deploy.sh` to include compiler service
- [ ] Update `justfile` with compiler-related targets

## Phase Dependency Graph

```
Phase 1 (WIT) ──┬──▶ Phase 2 (Host) ──▶ Phase 3 (Executor) ──▶ Phase 9 (Rust Guest)
                │                                                       │
                ├──▶ Phase 4 (TS SDK) ──▶ Phase 5 (Compiler) ──▶ Phase 10 (TS Guest)
                │                                                       │
Phase 6 (Artifact Store) ──▶ Phase 7 (PM Endpoints) ──▶ Phase 8 (Editor)
                                                                        │
Phase 12 (Infra) ───────────────────────────────────────────────▶ Phase 11 (E2E)
```

Phases 1, 4, 6, and 12 can start in parallel. Phase 11 is the final integration gate.

## Verification Checklist

| Check | Command |
|-------|---------|
| WIT valid | `wasm-tools component wit data-plane/oprc-wasm/wit/` |
| oprc-wasm builds | `cargo check -p oprc-wasm -q` |
| oprc-wasm tests pass | `cargo test -p oprc-wasm -q` |
| oprc-odgm builds | `cargo check -p oprc-odgm -q` |
| oprc-pm builds | `cargo check -p oprc-pm -q` |
| oprc-gui builds | `cargo check -p oprc-gui` |
| Compiler service works | `curl -X POST http://localhost:3000/compile -d '...'` |
| Existing WASM tests pass | `cargo test -p oprc-wasm -q` (old `oaas-function` guests) |
| Full workspace compiles | `cargo check --workspace -q` |
| System E2E passes | `just system-e2e` || Wasmtime↔ComponentizeJS compat | Compile trivial TS function → load in project's wasmtime → confirm instantiation |
| Module store capacity | Test LRU eviction under configurable limit |