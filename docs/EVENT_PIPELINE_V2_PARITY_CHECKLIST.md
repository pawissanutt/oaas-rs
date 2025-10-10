# Event Pipeline V2 Parity Checklist

This checklist tracks functional parity of Event Pipeline V2 versus the previous event system (direct Zenoh piping using ObjectEvent).

## 1) Trigger resolution and emission
- [x] Numeric triggers (data_trigger) matched
- [x] String triggers (data_trigger_str) matched
- [x] Per-entry action classification (Create/Update/Delete)
- [x] Idempotent delete suppression (no event when key absent)
- [x] Zenoh async emission via TriggerProcessor (fire-and-forget PUT)
- [x] TriggerProcessor injected when events are enabled and V2 flag is on
  - Acceptance: Any shard created via factory with events enabled and ODGM_EVENT_PIPELINE_V2=true emits triggers (injection occurs when an ObjectEvent exists; no-op otherwise)

## 2) Topic shapes and payloads
- [x] Stateless target topic: `oprc/<cls>/<partition>/async/<fn_id>/<invocation_id>`
- [x] Stateful object topic: `oprc/<cls>/<partition>/objects/<object_id>/async/<fn_id>/<invocation_id>`
- [x] Payload uses `TriggerPayload`/`EventInfo` with key or key_str context
- [~] Structured summary fallback emission with representative keys and counts
  - Status: Implemented as in-process structured summary attached to V2QueuedEvent; external Zenoh emission deferred
  - Acceptance: When fanout exceeds cap, attach a single summary with `changed_total`, `sample_keys`, `version_after`

## 3) Ordering and backpressure
- [x] Single consumer task per shard (FIFO ordering by `seq`)
- [x] Bounded queue with drop-on-full behavior
- [x] Counters: internal `queue_drops_total`, `emitted_total{type=create|update|delete,mode=per_entry}`
- [x] Counter: internal `emit_failures_total{reason=zenoh}`
- [x] Gauge (internal): `queue_len` (tracked as atomic; OTEL export pending)

## 4) Fanout control and summary fallback
- [x] Fanout cap enforcement via `ODGM_MAX_BATCH_TRIGGER_FANOUT`
- [x] Summary fallback (structured, in-process)
- [~] Structured summary external emission (see §2) – pending

## 5) Configuration and capability
- [x] `ODGM_EVENT_PIPELINE_V2` flag (auto-disables bridge)
- [ ] Capabilities RPC advertises `event_pipeline_v2=true`
- [ ] README/docs snippet for enabling V2 with key env vars

## 6) Event config persistence and retrieval
- [x] Persist `ObjectEvent` per object (record type 0x01)
- [x] Attach `event_config` to `MutationContext` for dispatcher
- [ ] Cache event_config to reduce repeated loads (optimization)

## 7) Metrics and observability (baseline parity)
- [x] `emitted_total{type=create|update|delete, mode=per_entry}` (internal counters)
- [x] `fanout_limited_total` (internal counter)
- [x] `queue_drops_total{reason}` (internal counter for queue_full)
- [x] `queue_len` (internal atomic/up-down)
- [x] `emit_failures_total{reason=zenoh}` (internal counter)
- [x] OTEL instruments wired; exporter activation pending environment
  - Enable via: OPRC_OTEL_METRICS_ENDPOINT=grpc://otel-collector:4317 (or http), optional OPRC_OTEL_SERVICE_NAME=oprc-odgm
  - Metrics exposed:
    - odgm.events.emitted.create.total | update.total | delete.total
    - odgm.events.fanout.limited.total
    - odgm.events.queue.drops.total
    - odgm.events.queue.len (UpDownCounter)
    - odgm.events.emit.failures.total

## 8) Test coverage for parity
- [x] Per-entry create/update/delete classification (T2)
- [x] Mixed batch with existing & new keys (T3)
- [x] Idempotent delete no event (T5)
- [x] Fanout cap → structured summary fallback (T4)
- [x] Mixed numeric + string triggers within one V2 batch (T6)
- [x] Ordering monotonicity per object (T7) – proxy via receipt count
- [x] Trigger publish failure counted (metric)
- [ ] Capability flag visible to client (T10)

## 9) Operational hardening
- [x] Graceful handling/logging for trigger publish errors (with counters)
- [ ] Throttled logging for drops/failures
- [ ] Test tap behind env flag and documented

Notes
- Legacy bridge-only tests are currently ignored to focus on J2.
- Minimal metrics implemented as internal counters; OTEL exporter integration to follow.
