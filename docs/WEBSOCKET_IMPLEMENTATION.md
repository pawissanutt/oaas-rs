# WebSocket Event Subscription — Design & Progress

> **Branch:** `dev` (commits `a8adde7` → `82e9753` → `bba36a0`)
> **Status:** Unit tests pass. System E2E **not yet validated** in a Kind cluster.

---

## 1. Goal

Enable real-time event streaming from ODGM mutations to clients via WebSocket, supporting three subscription granularities:

| Level | Route | Zenoh Topic Pattern |
|-------|-------|---------------------|
| Object | `GET /api/class/{cls}/{pid}/objects/{oid}/ws` | `oprc/{cls}/{pid}/events/{oid}` |
| Partition | `GET /api/class/{cls}/{pid}/ws` | `oprc/{cls}/{pid}/events/**` |
| Class | `GET /api/class/{cls}/ws` | `oprc/{cls}/*/events/**` |

**Data flow:** Client → WebSocket → Gateway (Zenoh subscriber) ← Zenoh ← ODGM (V2 event publisher)

---

## 2. Architecture

```
┌────────┐  WS   ┌─────────┐  Zenoh sub  ┌───────┐  Zenoh put  ┌──────┐
│ Client ├──────►│ Gateway  ├────────────►│ Zenoh ├◄────────────┤ ODGM │
└────────┘       └─────────┘             └───────┘             └──────┘
                  (axum WS)               (router)        (V2Dispatcher)
```

### Event payload (JSON, published to Zenoh by ODGM)

```rust
// Defined in data-plane/oprc-odgm/src/events/v2.rs
struct ZenohEventPayload {
    object_id: String,
    partition_id: u16,
    cls_id: String,
    changes: Vec<ChangeEntry>,   // action: Create/Update/Delete, key, value
    source: String,              // "Client" or "Sync"
}
```

### Gateway WS handler (core loop)

The `handle_ws()` function in `data-plane/oprc-gateway/src/handler/ws.rs`:
1. Splits the axum `WebSocket` into `(tx, rx)`.
2. Declares a Zenoh subscriber on the constructed topic.
3. Runs a `tokio::select!` loop:
   - **Zenoh sample received** → converts payload to UTF-8 string → sends as WS `Text` frame.
   - **WS message received** → handles `Close`/`Ping` or client disconnect → exits loop.
4. On exit, the Zenoh subscriber is dropped (auto-undeclares).

---

## 3. ODGM: Configurable Event Locality

### Problem

Events were hardcoded with `Locality::SessionLocal`, so they never left the ODGM process. The Gateway runs in a separate pod and couldn't receive them.

### Solution

Made locality configurable via env var with `Remote` as default.

### Files changed

- **`data-plane/oprc-odgm/src/shard/builder.rs`** — `EventPipelineConfig` struct
  - Added `zenoh_event_locality: zenoh::sample::Locality` field.
  - Parsed from `ODGM_ZENOH_EVENT_LOCALITY` env var. Values: `session_local`, `any`, anything else → `Remote` (default).
  - `disabled()` also includes the locality field (set to `Remote`).
  - Changed `zenoh_session` from `Option<Session>` to `Option<(Session, Locality)>` when passed to `make_v2_dispatcher_v2()`.

- **`data-plane/oprc-odgm/src/events/v2.rs`** — V2 event dispatcher
  - `new_with_zenoh()` signature: `zenoh_session: Option<(zenoh::Session, zenoh::sample::Locality)>`.
  - `run_v2_consumer()` uses `z_session.put(&topic, json).allowed_destination(locality).await`.

### Env vars

| Variable | Values | Default | Description |
|----------|--------|---------|-------------|
| `ODGM_ZENOH_EVENT_PUBLISH` | `true` / `false` | `false` | Enable Zenoh event publishing |
| `ODGM_ZENOH_EVENT_LOCALITY` | `session_local`, `any`, `remote` | `remote` | Zenoh publish locality |
| `ODGM_EVENT_PIPELINE_V2` | `true` / `false` | `true` | Enable V2 event pipeline |
| `ODGM_EVENT_QUEUE_BOUND` | integer | `1024` | Event queue capacity |
| `ODGM_EVENT_BCAST_BOUND` | integer | `256` | Broadcast channel capacity |
| `ODGM_MST_SYNC_EVENTS` | `true` / `false` | `false` | Emit events for MST sync writes |

---

## 4. Gateway: WS Route Registration

### Files changed

- **`data-plane/oprc-gateway/src/handler/ws.rs`** — Three handler functions:
  - `ws_object_handler` — `Path((cls, pid, oid))`
  - `ws_partition_handler` — `Path((cls, pid))`
  - `ws_class_handler` — `Path(cls)` (new in this work)

- **`data-plane/oprc-gateway/src/handler/mod.rs`** — Conditional registration:
  ```rust
  if ws_enabled {
      router = router
          .route("/api/class/{cls}/{pid}/objects/{oid}/ws", get(ws::ws_object_handler))
          .route("/api/class/{cls}/{pid}/ws", get(ws::ws_partition_handler))
          .route("/api/class/{cls}/ws", get(ws::ws_class_handler))
          .layer(Extension(z_session));
  }
  ```

- **`data-plane/oprc-gateway/src/conf.rs`** — `GATEWAY_WS_ENABLED` env var (already existed, supports `OPRC_GW_` prefix).

---

## 5. Tests

### 5.1 Unit Tests (PASS ✅)

File: `data-plane/oprc-gateway/tests/ws_routes_test.rs` — 4 tests, all passing.

| Test | What it verifies |
|------|-----------------|
| `ws_routes_return_404_when_disabled` | All 3 WS routes return 404 when `ws_enabled=false` |
| `ws_routes_registered_when_enabled` | All 3 WS routes respond (not 404) when `ws_enabled=true` |
| `ws_object_receives_zenoh_event` | Real TCP server + WS client; publishes Zenoh event → WS receives it |
| `ws_class_receives_events_from_multiple_partitions` | Class-level WS receives events from different partitions |

Run: `cargo test -p oprc-gateway -q` (12 total: 8 REST + 4 WS)

Dependencies added: `tokio-tungstenite` and `futures-util` as dev-deps in `data-plane/oprc-gateway/Cargo.toml`.

### 5.2 System E2E Tests (NOT YET VALIDATED ⏳)

File: `tests/system_e2e/src/main.rs` — Scenario 8 "WebSocket Event Subscription E2E Test"

**Gated by:** `OAAS_E2E_WS_ENABLED=true` environment variable.

**What it does:**
1. Finds the ODGM deployment via `kubectl get deploy -l oaas.io/class=E2EClass`.
2. Sets `ODGM_ZENOH_EVENT_PUBLISH=true` and `ODGM_ZENOH_EVENT_LOCALITY=remote` via `kubectl set env`.
3. Waits for ODGM rollout to complete.
4. **Test 1 — Object-level WS:** Connects WS client to `ws://localhost:30081/api/class/{cls}/0/objects/{id}/ws`, mutates the object via CLI, asserts the WS receives a JSON event containing the object ID.
5. **Test 2 — Class-level WS:** Connects WS to `/api/class/{cls}/ws`, mutates two objects, asserts both events received.

**Why `kubectl set env` instead of Helm values for ODGM?**
ODGM pods are created dynamically by the CRM controller (not directly by Helm). The CRM creates Deployments for ODGM based on `ClassRuntime` CRDs. So we can't set ODGM env vars in the Helm chart — we set them at runtime after CRM creates the ODGM deployment.

**Helm config:** `k8s/charts/examples/system-e2e-crm-1.yaml` has `gateway.extraEnv.GATEWAY_WS_ENABLED: "true"`.

Dependencies added: `tokio-tungstenite` and `futures-util` in `tests/system_e2e/Cargo.toml`.

---

## 6. Remaining Work

### Must do
- [ ] **Run system E2E in Kind cluster** — `OAAS_E2E_WS_ENABLED=true just system-e2e` to validate the full flow works end-to-end.
- [ ] **Fix any E2E failures** — The WS E2E test has not been run in a real cluster yet. Potential issues:
  - NodePort `30081` may not be correctly exposed for WS upgrade.
  - ODGM `kubectl set env` may require the deployment label `oaas.io/class` to exist (depends on CRM class registration).
  - Timing: WS connect may race with Zenoh subscriber readiness.

### Should do
- [ ] **ODGM env passthrough from CRM** — Consider adding a mechanism for the CRM controller to propagate ODGM env vars from the `ClassRuntime` CRD spec, so `ODGM_ZENOH_EVENT_PUBLISH` can be set declaratively instead of via `kubectl set env`.
- [ ] **WS auth/rate-limiting** — Currently WS endpoints have no authentication. For production use, add token-based auth or rate limiting.
- [ ] **WS heartbeat/ping** — The handler doesn't send periodic pings. Long-idle connections may be dropped by proxies. Consider adding a ping interval.

### Nice to have
- [ ] **Frontend WS client** — Build the Dioxus GUI or web frontend component that connects to the WS endpoints (see `TUTORIAL_PIXEL_CANVAS.md` for the pixel canvas use case).
- [ ] **Filtered subscriptions** — Allow clients to subscribe with filters (e.g., only specific keys or mutation types).

---

## 7. File Reference

| File | What changed |
|------|-------------|
| `Cargo.toml` (workspace) | Added `tokio-tungstenite = "0.28"` to workspace deps |
| `data-plane/oprc-odgm/src/shard/builder.rs` | `EventPipelineConfig` + locality field, `from_env()` parsing |
| `data-plane/oprc-odgm/src/events/v2.rs` | `new_with_zenoh()` accepts `(Session, Locality)`, uses `allowed_destination()` |
| `data-plane/oprc-gateway/src/handler/ws.rs` | 3 WS handlers: object, partition, class |
| `data-plane/oprc-gateway/src/handler/mod.rs` | Conditional WS route registration |
| `data-plane/oprc-gateway/Cargo.toml` | `tokio-tungstenite`, `futures-util` dev-deps |
| `data-plane/oprc-gateway/tests/ws_routes_test.rs` | 4 unit tests (route + message delivery) |
| `tests/system_e2e/Cargo.toml` | `tokio-tungstenite`, `futures-util` deps |
| `tests/system_e2e/src/main.rs` | Scenario 8: WS E2E test with `kubectl set env` |
| `k8s/charts/examples/system-e2e-crm-1.yaml` | `GATEWAY_WS_ENABLED: "true"` |

---

## 8. How to Test

```bash
# Unit tests (should all pass)
cargo test -p oprc-gateway -q

# System E2E (requires Kind cluster + built images)
just system-e2e                           # without WS tests
OAAS_E2E_WS_ENABLED=true just system-e2e  # with WS tests

# Manual WS test (if gateway is running locally or port-forwarded)
# Install websocat: cargo install websocat
websocat ws://localhost:30081/api/class/my-pkg.MyClass/ws
# In another terminal, mutate an object to trigger an event
```
