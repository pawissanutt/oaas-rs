# Local Dev Server Design

## Problem

Developing OaaS applications today requires deploying a full Kubernetes cluster (Kind + Helm charts + multiple services). The feedback loop for WASM function iteration and object data inspection is slow. We need a single-binary local dev server that gives developers a production-faithful environment without the infrastructure overhead.

## Goals

1. **Single command**: `oprc dev serve` starts everything locally.
2. **Real shard logic**: Use actual ODGM in-memory shards (not mocks) for production-faithful object storage, granular entries, and cross-object invocations.
3. **WASM execution**: Load and run WASM components the same way ODGM does in production.
4. **Gateway API**: Expose the same REST/gRPC routes as `oprc-gateway` — existing clients and SDKs work unchanged.
5. **Web UI**: Embed the `oprc-next` frontend for object browsing and management without a separate process.
6. **Multi-collection**: Support multiple classes/collections in one server, so applications like pixel-canvas (with `PixelRecord` + `GameOfLife`) work locally.
7. **Maximum code reuse**: Compose existing crates; avoid reimplementing data paths.

## Non-Goals

- Raft/MST replication (single-node only).
- Kubernetes CRD reconciliation.
- Package Manager deployment workflows (collections are configured via YAML).
- Production performance tuning.
- Hot-reload of WASM modules (restart the server to pick up changes).

---

## Architecture

### Zenoh Loopback (Option B)

The key design decision is **how the gateway communicates with shards**. We use a Zenoh session in `peer` mode with no external peers — a fully in-process message bus:

```
┌─────────────────────────────────────────────────────────┐
│  oprc dev serve                                         │
│                                                         │
│  ┌───────────┐   Zenoh queries   ┌──────────────────┐   │
│  │  Gateway   │ ───────────────► │  ODGM Shards     │   │
│  │  (REST +   │                  │  (queryables)    │   │
│  │   gRPC)    │ ◄─────────────── │                  │   │
│  └───────────┘   Zenoh replies   │  ┌────────────┐  │   │
│        │                         │  │ WASM Exec  │  │   │
│        │                         │  └────────────┘  │   │
│  ┌─────┴─────┐                   └──────────────────┘   │
│  │  Axum     │                                          │
│  │  Router   │                                          │
│  │  ┌──────┐ │                                          │
│  │  │Static│ │  ← embedded oprc-next (rust-embed)       │
│  │  │Files │ │                                          │
│  │  └──────┘ │                                          │
│  └───────────┘                                          │
│                                                         │
│  Zenoh Session (peer mode, no external peers)           │
└─────────────────────────────────────────────────────────┘
```

**Why Zenoh loopback?**
- Gateway handlers (`rest.rs`) use `Extension<ObjectProxy>` which queries Zenoh key expressions. They work unchanged.
- ODGM shards register Zenoh queryables in `build()` (the networked builder). Cross-collection invocations (e.g., `this.sibling()` in WASM) route through Zenoh naturally.
- Already validated by `data-plane/oprc-gateway/tests/rest_basic.rs`, which opens a peer-mode Zenoh session and mounts queryables for testing.

### What We Reuse (Unchanged)

| Component | Crate | What |
|-----------|-------|------|
| REST/gRPC handlers | `oprc-gateway` | `build_router()` — mounts all `/api/class/...` routes |
| Object proxy | `oprc-invoke` | `ObjectProxy::new(session)` — queries Zenoh |
| Shard builder | `oprc-odgm` | `ShardBuilder::new().metadata(m).memory_storage()?.no_replication().build()` (networked variant) |
| WASM bridge | `oprc-odgm` | `setup_wasm_offloader()` — detects `wasm://` routes, creates wasmtime executor |
| WASM runtime | `oprc-wasm` | `WasmInvocationExecutor`, `WasmExecutorAdapter`, `ShardDataOpsFactory` |
| Data grid manager | `oprc-odgm` | `start_raw_server()` — creates Pool, MetaManager, ShardManager, watch stream |
| Collection creation | `oprc-odgm` | `create_collection()` — parses JSON config into `CreateCollectionRequest` |
| Zenoh pool | `oprc-zenoh` | `Pool::new()` with default peer config |

### What We Build (~300 lines)

1. **Dev server main loop** (`oprc-dev` or inline in `oprc-cli`): Parse YAML config → start ODGM → create collections → set up WASM offloaders → start gateway router → mount static files → serve.
2. **YAML config parser**: Translate developer-friendly config into `CreateCollectionRequest` + WASM module paths.
3. **Static file serving**: Embed `oprc-next` build output and serve via `tower-http::ServeDir` (or `rust-embed` handler).
4. **Stub PM API endpoints**: Minimal `/api/v1/deployments`, `/api/v1/packages` handlers so the UI can populate class selectors.

---

## Configuration Format

Developers write a `oaas-dev.yaml` (or pass `--config <path>`):

```yaml
# oaas-dev.yaml
port: 8080

collections:
  - name: "PixelRecord"
    partition_id: 0
    functions:
      - id: "randomColor"
        wasm: "./target/wasm32-wasip2/release/pixel_func.wasm"
      - id: "setColor"
        wasm: "./target/wasm32-wasip2/release/pixel_func.wasm"

  - name: "GameOfLife"
    partition_id: 0
    functions:
      - id: "golStep"
        wasm: "./target/wasm32-wasip2/release/gol_func.wasm"
```

The dev server translates each collection entry into:
- A `CreateCollectionRequest` with partition/shard metadata
- `ShardMetadata.invocations.fn_routes` entries with `wasm://` URLs
- WASM module loading via `setup_wasm_offloader()`

---

## Startup Sequence

```
1. Parse oaas-dev.yaml
2. Create Zenoh session (peer mode, no external peers)
3. Create ODGM via start_raw_server()
   └── Pool, MetaManager, ShardManager, watch stream
4. For each collection in config:
   a. Build CreateCollectionRequest with fn_routes
   b. metadata_manager.create_collection(req)
   c. Wait for shard to be created (watch stream)
   d. setup_wasm_offloader() on the shard
   e. shard.set_local_offloader(wasm_adapter)
5. Build gateway router:
   a. oprc_gateway::build_router(session, timeout, ws_enabled)
   b. Mount stub PM API routes (/api/v1/deployments, etc.)
   c. Mount embedded static files as fallback
6. Bind to port and serve
```

---

## Frontend Embedding

### Strategy: `rust-embed`

The `oprc-next` frontend produces a static export (`output: "export"` in `next.config.ts`) into `frontend/oprc-next/out/`. We embed this directory into the binary at compile time using `rust-embed`:

```rust
#[derive(rust_embed::Embed)]
#[folder = "../../frontend/oprc-next/out"]
struct FrontendAssets;
```

An Axum handler serves these embedded files as the fallback route:

```rust
async fn serve_frontend(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match FrontendAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        // SPA fallback: serve index.html for client-side routes
        None => match FrontendAssets::get("index.html") {
            Some(content) => {
                ([(header::CONTENT_TYPE, "text/html")], content.data).into_response()
            }
            None => StatusCode::NOT_FOUND.into_response(),
        }
    }
}
```

**Build requirement**: Run `npm run build` (or `npx next build`) in `frontend/oprc-next/` before `cargo build` with the dev-server feature. The CI/justfile target should handle this automatically.

### Stub PM API

The `oprc-next` frontend expects certain PM API endpoints to populate its UI (class list, deployment list, etc.). The dev server provides minimal stub implementations:

| Endpoint | Response |
|----------|----------|
| `GET /api/v1/deployments` | JSON array of deployment objects derived from the YAML config |
| `GET /api/v1/packages` | JSON array of package descriptors (one per collection) |
| `GET /api/v1/envs` | Empty array `[]` |

These are simple static responses built from the parsed config — no actual PM logic.

### Gateway Route Mapping

The `oprc-next` frontend calls `/api/gateway/api/class/{cls}/{pid}/objects/{oid}` (it was designed to be proxied through PM). In the dev server, we mount the gateway routes directly and add a redirect/rewrite layer:

```rust
// Direct gateway routes (from oprc-gateway)
// /api/class/{cls}/{pid}/objects/{oid}

// Compatibility routes (for oprc-next frontend)
// /api/gateway/* → strip prefix → forward to gateway routes
```

---

## CLI Integration

### Subcommand: `oprc dev`

Add a `Dev` variant to `OprcCommands` in `tools/oprc-cli/src/types.rs`:

```rust
#[derive(Subcommand)]
pub enum OprcCommands {
    // ... existing variants ...

    /// Local development server
    #[cfg(feature = "dev-server")]
    Dev(DevArgs),
}

#[derive(Args)]
pub struct DevArgs {
    #[command(subcommand)]
    pub command: DevCommands,
}

#[derive(Subcommand)]
pub enum DevCommands {
    /// Start the local dev server
    Serve {
        /// Path to oaas-dev.yaml config file
        #[arg(short, long, default_value = "oaas-dev.yaml")]
        config: PathBuf,

        /// Port to listen on (overrides config)
        #[arg(short, long)]
        port: Option<u16>,
    },
}
```

### Feature Flag: `dev-server`

In `tools/oprc-cli/Cargo.toml`:

```toml
[features]
default = []
dev-server = [
    "dep:oprc-odgm",
    "dep:oprc-gateway",
    "dep:oprc-wasm",
    "dep:wasmtime",
    "dep:tower-http",
    "dep:rust-embed",
    "dep:mime_guess",
]

[dependencies]
# ... existing deps ...

# Dev server (optional)
oprc-odgm = { path = "../../data-plane/oprc-odgm", features = ["wasm"], optional = true }
oprc-gateway = { path = "../../data-plane/oprc-gateway", optional = true }
oprc-wasm = { path = "../../data-plane/oprc-wasm", optional = true }
wasmtime = { workspace = true, optional = true }
tower-http = { workspace = true, features = ["fs"], optional = true }
rust-embed = { version = "8", optional = true }
mime_guess = { version = "2", optional = true }
```

### Install Command

```bash
# Install CLI with dev server support
cargo install --path tools/oprc-cli --features dev-server

# Or via justfile
just install-dev-tools
```

---

## Data Flow Examples

### Object CRUD

```
Client PUT /api/class/PixelRecord/0/objects/pixel_1
  → Gateway REST handler (put_obj)
    → ObjectProxy.set(key_expr, data)
      → Zenoh query to "oprc/PixelRecord/0/objects/pixel_1/set"
        → Shard queryable receives query
          → shard.set_entry_granular("pixel_1", "_raw", data)
            → In-memory skiplist storage
          → Zenoh reply "ok"
        → ObjectProxy returns success
      → HTTP 200
```

### WASM Invocation

```
Client POST /api/class/PixelRecord/0/objects/pixel_1/invokes/randomColor
  → Gateway REST handler (invoke_obj)
    → ObjectProxy.invoke_obj(key_expr, request)
      → Zenoh query to "oprc/PixelRecord/0/objects/pixel_1/invokes/randomColor"
        → Shard queryable receives query
          → InvocationOffloader.invoke_obj()
            → WasmExecutorAdapter.invoke_obj()
              → WasmInvocationExecutor.call_component()
                → wasmtime runs pixel_func.wasm
                  → WASM calls host get_object/set_object
                    → ShardDataOpsAdapter → shard storage
                → returns result
              → Zenoh reply with InvocationResponse
            → ObjectProxy returns response
          → HTTP 200 with result payload
```

### Cross-Collection Invocation (GameOfLife → PixelRecord)

```
WASM (golStep) calls host: invoke_obj("PixelRecord", 0, "pixel_1", "setColor", data)
  → ShardDataOpsAdapter.invoke_obj()
    → shard.invoke_obj(ObjectInvocationRequest { cls_id: "PixelRecord", ... })
      → InvocationOffloader routes via Zenoh (different collection)
        → PixelRecord shard queryable handles it
          → WasmExecutorAdapter.invoke_obj() on PixelRecord's shard
            → runs pixel_func.wasm setColor
```

This works because both shards share the same Zenoh session. The `ObjectProxy` inside the shard's `InvocationOffloader` routes via Zenoh key expressions — it doesn't need to know if the target is local or remote.

---

## What's Omitted and Why

| Feature | Why Omitted |
|---------|-------------|
| Replication (Raft/MST) | Single-node dev; would add complexity without benefit |
| Package Manager | No deploy workflow needed; collections configured directly via YAML |
| CRM / K8s controller | No Kubernetes in local dev |
| OpenTelemetry export | Could be added later; not needed for basic dev loop |
| Persistent storage | In-memory only; dev server is ephemeral. Could add RocksDB later |
| WASM hot-reload | Restart server to pick up changes; hot-reload is a future enhancement |
| Multi-node / clustering | Single process; not useful for local dev |

---

## Implementation Phases

### Phase 1: Core Server (MVP)
- Parse YAML config
- Start ODGM with in-memory shards (no WASM yet)
- Mount gateway routes
- Object CRUD works via REST API
- Add `oprc dev serve` CLI subcommand

### Phase 2: WASM Integration
- Wire `setup_wasm_offloader()` for each collection
- Load WASM modules from local file paths
- Function invocation works end-to-end
- Cross-collection invocation works

### Phase 3: Frontend
- Embed `oprc-next` static export via `rust-embed`
- Add stub PM API endpoints
- Add `/api/gateway/*` prefix rewrite for frontend compatibility
- Web UI accessible at `http://localhost:8080/`

### Phase 4: Developer Experience Polish
- `oprc dev init` — scaffold `oaas-dev.yaml` from project structure
- Watch mode for YAML config changes (restart shards)
- Pretty startup banner showing loaded collections and routes
- Error messages with actionable suggestions (e.g., "WASM module not found at ./target/...")

---

## Testing Strategy

- **Unit tests**: YAML config parsing, stub API responses.
- **Integration test**: Start dev server in-process, send HTTP requests, verify object CRUD and WASM invocation. Similar pattern to `rest_basic.rs` — no external dependencies needed.
- **Manual validation**: Run pixel-canvas against the dev server instead of a Kind cluster.

---

## Prior Art / Validation

The Zenoh loopback approach is already proven in:
- `data-plane/oprc-gateway/tests/rest_basic.rs` — opens peer-mode Zenoh, registers queryables, tests gateway REST handlers via `tower::ServiceExt::oneshot()`.
- `data-plane/oprc-odgm/tests/local_invocation_test.rs` — builds minimal shards with `LocalFnOffloader`, tests local function invocation.
- `data-plane/oprc-odgm/tests/wasm_integration_test.rs` — builds shards with WASM offloaders, tests WASM execution end-to-end.

The dev server essentially composes these patterns into a long-running single binary.
