# Scripting Runtime — Implementation TODO

> Phases for implementing the OOP scripting layer described in [SCRIPTING_RUNTIME_DESIGN.md](SCRIPTING_RUNTIME_DESIGN.md).
> Dependencies between phases are noted; independent phases can run in parallel.

## Phase 1: OOP WIT Interface (`oprc-wasm`)

> Prerequisite: None. Builds on existing `data-plane/oprc-wasm/wit/oaas.wit`.

- [ ] Design the `object-context` interface in WIT
  - [ ] `get-self() → result<obj-data, odgm-error>`
  - [ ] `set-self(data: obj-data) → result<_, odgm-error>`
  - [ ] `get-field(key: string) → result<option<list<u8>>, odgm-error>`
  - [ ] `set-field(key: string, value: list<u8>) → result<_, odgm-error>`
  - [ ] `delete-field(key: string) → result<_, odgm-error>`
  - [ ] `get-object(object-id: string) → result<obj-data, odgm-error>`
  - [ ] `invoke(target-id: string, fn-name: string, payload: option<list<u8>>) → result<list<u8>, odgm-error>`
  - [ ] `log(level: log-level, message: string)` with `log-level` enum (debug, info, warn, error)
- [ ] Design the `guest-object` interface (exports)
  - [ ] `on-invoke(function-name: string, payload: option<list<u8>>, headers: list<key-value>) → invocation-response`
- [ ] Define new WIT world `oaas-object` (imports `object-context`, exports `guest-object`)
- [ ] Verify WIT compiles: `wasm-tools component wit data-plane/oprc-wasm/wit/`
- [ ] Add `bindgen!` for the new world in `oprc-wasm/src/lib.rs` (alongside existing `oaas-function` bindings)
- [ ] Verify crate compiles: `cargo check -p oprc-wasm -q`

## Phase 2: Host Implementation for `object-context` (`oprc-wasm`)

> Prerequisite: Phase 1.

- [ ] Extend `WasmHostState` to store invocation context (`cls_id`, `partition_id`, `object_id`)
  - Note: these are already available from the `InvocationRequest` passed to the executor
- [ ] Implement `object-context` host trait on `WasmHostState`
  - [ ] `get-self` → delegate to `OdgmDataOps::get_object` with stored context
  - [ ] `set-self` → delegate to `OdgmDataOps::set_object` with stored context
  - [ ] `get-field` → delegate to `OdgmDataOps::get_value` with stored context
  - [ ] `set-field` → delegate to `OdgmDataOps::set_value` with stored context
  - [ ] `delete-field` → delegate to `OdgmDataOps::delete_value` with stored context
  - [ ] `get-object` → delegate to `OdgmDataOps::get_object` (resolve class/partition from context)
  - [ ] `invoke` → delegate to `OdgmDataOps::invoke_fn`
  - [ ] `log` → route to `tracing::{debug,info,warn,error}!` macros with guest source label
- [ ] Unit tests for each host function
- [ ] Verify: `cargo check -p oprc-wasm -q`

## Phase 3: Executor Updates (`oprc-wasm`)

> Prerequisite: Phase 2.

- [ ] Add world detection in `WasmInvocationExecutor`
  - [ ] Check which exports are present (`guest-function` vs `guest-object`) after component instantiation
  - [ ] Store world type per compiled module (enum: `Legacy` / `ObjectOriented`)
- [ ] For `oaas-object` guests: map `invoke_fn` and `invoke_obj` calls to `on-invoke`
  - [ ] Set invocation context in `WasmHostState` before calling guest
  - [ ] Map `fn_id` → `function-name` parameter
- [ ] For `oaas-function` guests: preserve existing behavior (unchanged)
- [ ] Update `WasmExecutorAdapter` to handle both world types
- [ ] Integration test: load a guest targeting `oaas-object` world → invoke → verify host context works
- [ ] Verify: `cargo test -p oprc-wasm -q`

## Phase 4: TypeScript SDK (`@oaas/sdk`)

> Prerequisite: Phase 1 (WIT definition). Can run in parallel with Phase 2-3.

- [ ] Create directory: `sdk/typescript/` (or `tools/oaas-sdk-ts/`)
- [ ] Initialize npm package: `@oaas/sdk`
- [ ] Implement `OaaSObject` abstract base class
  - [ ] `get<T>(key: string): Promise<T | null>` — calls `object-context.get-field`, deserializes JSON
  - [ ] `set<T>(key: string, value: T): Promise<void>` — serializes JSON, calls `object-context.set-field`
  - [ ] `delete(key: string): Promise<void>` — calls `object-context.delete-field`
  - [ ] `getSelf(): Promise<ObjData>` — calls `object-context.get-self`
  - [ ] `invoke(targetId: string, fn: string, payload?: any): Promise<any>` — calls `object-context.invoke`
- [ ] Implement `Response` helper class
  - [ ] `Response.ok(data?: any): InvocationResponse`
  - [ ] `Response.error(message: string): InvocationResponse`
  - [ ] `Response.notFound(): InvocationResponse`
- [ ] Implement method dispatch shim (entry point that ComponentizeJS compiles)
  - [ ] Import user's default export class
  - [ ] Instantiate it
  - [ ] Wire `guest-object.on-invoke` → look up method by `function-name` → call it on instance
  - [ ] Handle missing methods → return 404 response
- [ ] Write TypeScript type declarations (`index.d.ts`) for Monaco IntelliSense
- [ ] Unit tests for serialization/deserialization helpers
- [ ] Write a sample guest: `sdk/typescript/examples/counter.ts`

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
  - [ ] `POST /compile` → accepts `{ source, language }` → returns `{ success, wasm?, errors? }`
  - [ ] `GET /health` → returns `{ status: "ok" }`
- [ ] Error handling: TypeScript type errors, compilation failures, OOM protection
- [ ] Write Dockerfile (`tools/oprc-compiler/Dockerfile`)
  - [ ] Base: Node.js 20 slim
  - [ ] Pre-install dependencies
  - [ ] Copy WIT files from `data-plane/oprc-wasm/wit/`
- [ ] Manual test: compile a sample TypeScript function → verify output is valid WASM Component
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
  - [ ] Pipeline: compile → store artifact → create/update OPackage → create OClassDeployment
  - [ ] Returns: `{ deployment_key, artifact_url, status }`
- [ ] Integration test: submit source → verify package + deployment created with correct `wasm_module_url`
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

- [ ] Update `tests/wasm-guest-echo/` to target the new `oaas-object` world
  - [ ] Implement `guest-object.on-invoke` instead of `guest-function.invoke-fn`/`invoke-obj`
  - [ ] Use `object-context.get-self`/`set-self` instead of explicit `data-access` calls
- [ ] Keep old test binary for backward-compatibility validation
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
| System E2E passes | `just system-e2e` |
