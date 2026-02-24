# WASM Runtime Integration Design

## Overview

OaaS-RS currently executes user functions by making gRPC calls to external
containers managed by the Control Plane (CRM).  WASM runtime integration adds
an **in-process execution path**: WebAssembly modules can be loaded and run
directly inside ODGM, eliminating the network round-trip for latency-sensitive
or simple functions.

---

## Architecture

### Current Architecture (gRPC only)

```
Client → Gateway → ODGM → InvocationOffloader → gRPC pool → Function Container
```

### Target Architecture (gRPC + Local/WASM)

```
Client → Gateway → ODGM ─┬→ LocalFnOffloader  → (in-process) Rust/WASM handler
                          └→ InvocationOffloader → gRPC pool → Function Container
```

The shard checks `local_offloader` first; if present, it handles the call
in-process.  Otherwise it falls back to the existing `inv_offloader` (gRPC).

---

## Core Abstractions

### `InvocationExecutor` (existing, `commons/oprc-invoke/src/handler.rs`)

```rust
#[async_trait]
pub trait InvocationExecutor {
    async fn invoke_fn(req: InvocationRequest)     -> Result<InvocationResponse, OffloadError>;
    async fn invoke_obj(req: ObjectInvocationRequest) -> Result<InvocationResponse, OffloadError>;
}
```

Both the existing `InvocationOffloader` (gRPC) and the new `LocalFnOffloader`
implement this trait.

### `LocalFnOffloader` (new, `commons/oprc-invoke/src/local.rs`)

A registry mapping `fn_id → Rust closure`.  The closure receives the raw
protobuf `payload` bytes and returns raw response bytes.

```rust
pub struct LocalFnOffloader {
    fn_handlers:  HashMap<String, FnHandlerFn>,  // stateless fn
    obj_handlers: HashMap<String, FnHandlerFn>,  // object method (falls back to fn_handlers)
}
pub type FnHandlerFn = Arc<dyn Fn(Vec<u8>) -> Vec<u8> + Send + Sync>;
```

### Attachment to `ObjectUnifiedShard`

```rust
pub struct ObjectUnifiedShard<A, R, E> {
    // ... existing fields ...
    /// Optional in-process executor.  Checked before the remote gRPC offloader.
    pub(crate) local_offloader:
        Option<Arc<dyn InvocationExecutor + Send + Sync>>,
}
```

Builder method:

```rust
let shard = shard.with_local_offloader(Arc::new(my_offloader));
```

---

## WASM Integration Plan (Phase 2, not yet implemented)

A `WasmFnOffloader` (using `wasmtime`) will implement `InvocationExecutor` and
store pre-compiled `wasmtime::Module` instances per `fn_id`.

Host/WASM ABI contract:

| Direction    | Convention                              |
|--------------|-----------------------------------------|
| Host → WASM  | `invoke(ptr: i32, len: i32) → i32`      |
| Return value | pointer to `(len: u32, data: [u8; len])` |

The host writes the payload into WASM linear memory, calls `invoke`, then reads
the result from linear memory.

---

## Configuration (future)

When the function route URL uses the `wasm://` scheme, the shard builder
creates a `WasmFnOffloader`:

```json
{
  "invocations": {
    "fn_routes": {
      "my_fn": { "url": "wasm:///opt/functions/my_fn.wasm", "stateless": true }
    }
  }
}
```

---

## Data Flow (local invocation)

```
1. ODGM receives InvokeFn / InvokeObj (gRPC or Zenoh)
2. ObjectShard::invoke_fn() checks self.local_offloader
3a. local_offloader present → LocalFnOffloader::invoke_fn() → closure(payload) → response
3b. absent              → InvocationOffloader::invoke_fn() → gRPC → function container
4. Response returned to caller
```

---

## Testing

- **Unit tests** (`commons/oprc-invoke/src/local.rs`): echo, transform, error,
  fallback, obj-vs-fn handler precedence.
- **Integration tests** (`data-plane/oprc-odgm/tests/local_invocation_test.rs`):
  minimal shard + `LocalFnOffloader`, invoke via `ObjectShard` trait methods.
- **E2E tests** (future): deploy a WASM module, invoke via CLI, verify output.

---

## Security

- WASM sandboxing prevents arbitrary host-memory access.
- `wasmtime` provides capability-based WASI; only explicitly granted
  capabilities are exposed to the module.
- Binary signing/integrity checks should be enforced before loading.
