% Event Pipeline V2 Design
% Status: Draft (Active – Bridge complete, V2 scaffolding in progress)
% Last Updated: 2025-10-06 (Post bridge broadcast + mutation context scaffolding)

## 1. Goal & Summary
Deliver a lightweight, per-entry aware event emission pipeline that replaces the removed legacy object‑blob diff logic. Initial step (Bridge/J0) restores green tests with minimal object-level events. Core V2 (J1+) provides efficient per-entry create/update/delete events without full object reconstruction, includes fanout limiting, and supports both numeric and string entry keys using existing proto (`ObjectEvent`, `TriggerPayload`, `EventInfo`). Design keeps storage and event layers loosely coupled and avoids premature complexity (no CRDT merge awareness yet).

## 2. Problems in Previous Version (Legacy Pipeline)
| Issue | Impact | Root Cause | V2 Resolution |
|-------|--------|------------|---------------|
| Full object reconstruction on every write | High CPU / latency for large objects | Blob-centric diff (scan all entries) | Use mutation context listing only touched keys |
| No per-entry granularity for string keys | Missed trigger precision | Logic keyed only by numeric map | Unified key normalization (all keys as canonical string) |
| Unbounded trigger fanout | Risk of overload / storm | No limit & no batching | Configured fanout cap + summary fallback |
| Duplicate update vs create vs delete classification errors | Incorrect event semantics | Lacked old/new presence check per key | Presence map + old value existence check before commit |
| Coupling to blob serialization | Fragile after granular migration | Reconstruction necessity | Operate on mutation metadata pre/post write (no blob) |
| Blocking emission in write path | Increased tail latency | Synchronous trigger exec | Async queue + background dispatcher |
| No backpressure metrics | Silent drops possible (future) | No queue metrics | Explicit bounded channel + drop counters |
| Idempotent delete emitted delete event repeatedly | Noise | No state check | Emit only if key existed pre-batch |
| Hard to reason about ordering guarantees | Race conditions | Mixed sync/async operations | Deterministic ordering: commit -> enqueue -> emit in sequence number order |

## 3. Scope
In Scope (V2 Core):
- Capture changed keys & classify create/update/delete without scanning entire object.
- Trigger evaluation using existing `ObjectEvent` (numeric + string maps).
- Fanout limiting & summary fallback.
- Asynchronous dispatch via bounded channel.
- Minimal metrics & configuration flags.
- Backward compatible use of existing proto payloads.

Out of Scope (Future / Enhancements):
- CRDT semantic diff (merge vs overwrite detection beyond create/update/delete).
- Cross-object aggregation or streaming window coalescing (beyond simple micro-batch option).
- Persistence / replay of missed events (at-most-once delivery only initially).
- Multi-tenant QoS scheduling.

## 4. Functional Requirements
1. Emit object-level summary event for each successful batch mutation (Bridge mode) OR per-entry events (V2 mode) within configurable fanout limits.
2. Classify each touched key into one of: CREATE, UPDATE, DELETE.
3. Support both numeric and string entry triggers defined in `ObjectEvent` (`data_trigger` and `data_trigger_str`).
4. Respect fanout cap: if total potential emissions > cap, emit a single summary event (scope="object") listing up to N representative keys and a truncated count.
5. Never emit DELETE for keys that were already absent (idempotent delete).
6. Provide monotonic ordering per object (events for later version never emitted before earlier version for same object) – best effort using single dispatcher task per shard.
7. Emission must not block write latency beyond a small enqueue cost (< 50µs target).

## 5. Non-Functional Requirements
- Low overhead: O(k) where k = number of touched keys, not O(total_object_entries).
- Memory bounded: queue size configurable; backpressure results in controlled drops + metrics.
- Simplicity: minimal new types; leverage existing proto messages.
- Observability: counters + histograms for latency and drops.

## 6. Key Concepts & Data Structures
```
MutationContext {
	object_id: ObjectIdRef,             // numeric or string
	cls_id: String,
	partition_id: u32,
	version_before: u64,
	version_after: u64,
	changed: Vec<ChangedKey>,           // as provided by batch_set_entries
}

ChangedKey {
	key_canonical: String,              // numeric encoded as decimal string
	action: MutAction,                  // Create | Update | Delete (filled post old-value check)
}

QueuedEvent {
	seq: u64,                           // per-shard increment
	ctx: MutationContext,
	mode: EventEmissionMode,            // BridgeSummary | PerEntry
}
```

Derivation of `action`:
```
if op.delete && old_value.exists -> Delete
else if op.delete && !old_value.exists -> (drop – no event)
else if old_value.exists -> Update
else -> Create
```

## 7. Flow Overview
Write Path (BatchSetValues / SetValue):
1. Collect mutations (list of (key, maybe_delete, new_value_bytes)).
2. Pre-fetch old presence for those keys (point lookups; skip values if not needed) to classify actions.
3. Apply batch atomically (replication commit).
4. Build `MutationContext` (version_before, version_after, changed[]).
5. Enqueue `QueuedEvent` into shard event channel (non-blocking, drop if full with metric increment).
6. Dispatcher task consumes queue, performs trigger matching + emission.

Dispatcher Emission:
1. Resolve object event config (metadata fetch). If absent → emit only summary (if Bridge) or skip if no triggers registered.
2. If Bridge mode: construct single summary event with aggregated key lists (capped) and counts.
3. Else (V2 mode): For each changed key, evaluate triggers (numeric or string map). Emit per-entry events for matched triggers.
4. Always optionally emit object summary if configured triggers require object-level notifications (future extension; initially off).

## 8. Modes & Flags
| Flag | Purpose | Default |
|------|---------|---------|
| `ODGM_EVENT_PIPELINE_BRIDGE` | Enable J0 bridge summary emission | true (until V2 stable) |
| `ODGM_EVENT_PIPELINE_V2` | Enable per-entry (disable bridge) | false |
| `ODGM_EVENT_COALESCE_WINDOW_MS` | Micro-batching wait window (0 = disabled) | 0 |
| `ODGM_MAX_BATCH_TRIGGER_FANOUT` | Cap on per-entry events per batch | 256 |
| `ODGM_EVENT_QUEUE_BOUND` | Bounded channel size per shard | 1024 |
| `ODGM_EVENT_SUMMARY_KEY_SAMPLE` | Max keys listed in summary | 32 |

Precedence: If `ODGM_EVENT_PIPELINE_V2=true` → per-entry mode; bridge disabled automatically.

## 9. Emission Topics (Zenoh)
Reuse reference patterns (see `reference.adoc`):
- Stateful trigger: `oprc/<cls>/<partition>/objects/<object_id>/async/<fn_id>/<invocation_id>`
- Stateless trigger: `oprc/<cls>/<partition>/async/<fn_id>/<invocation_id>`

`invocation_id`: `evt-<timestamp>-<seq>-<random8>`

## 10. Payloads
Use existing `TriggerPayload`:
```
TriggerPayload {
	event_info: EventInfo {
		 source_cls_id,
		 source_partition_id,
		 source_object_id (numeric if available else 0; put string form into context map),
		 event_type,                      // Mapped from action
		 key (numeric field only when numeric original),
		 timestamp,
		 context: { "object_id_str"?: ..., "key": <canonical>, "version": <version_after>, "summary_truncated"?: bool, "changed_total"?: N }
	},
	original_payload: <optional future value; omitted V2>
}
```
EventType mapping:
- Create → DATA_CREATE
- Update → DATA_UPDATE
- Delete → DATA_DELETE
Summary event uses `DATA_UPDATE` (or introduce new internal code later) with extra context keys.

## 11. Trigger Matching Algorithm (Per-Entry Mode)
For each `ChangedKey` with action ≠ dropped:
1. If key parses as u32 and exists in `data_trigger`: evaluate actions (on_create/on_update/on_delete) list.
2. Else evaluate `data_trigger_str` map with canonical key.
3. For each `TriggerTarget` in matched list for the specific action, enqueue emission (fanout counter increment). Stop if fanout cap reached; then produce summary event for remainder (if not yet emitted) and break.

## 12. Fanout Handling
`remaining = cap - emitted_entries`.
- If `remaining <= 0`: emit (or update) a single summary event capturing counts of suppressed events.
- Summary event includes `changed_total` and `summary_truncated=true`.
- Optional: Provide sample first `ODGM_EVENT_SUMMARY_KEY_SAMPLE` keys.

## 13. Backpressure & Queueing
- Bounded `tokio::mpsc::channel(ODGM_EVENT_QUEUE_BOUND)` per shard.
- On `try_send` failure (full): increment `odgm_event_queue_drops_total{reason="queue_full"}`; log at debug (throttle warn every X seconds).
- No retry (at-most-once semantics). Simplicity over guaranteed delivery.

## 14. Concurrency & Ordering
- Single dispatcher task per shard ensures FIFO order by `seq` (monotonic counter at enqueue time).
- Optional coalescing: if window > 0 ms: dispatcher collects contexts until window expires or queue empty; merges changed keys (later version wins per key) before emission. (Disabled by default for simplicity.)

## 15. Error Handling
- Emission failure (Zenoh publish error): increment `odgm_event_emit_failures_total` and continue; no retries.
- Trigger config decode errors: increment `odgm_event_trigger_decode_errors_total`; skip problematic triggers.
- Clock issues (timestamp read fail – unlikely): fallback to system time 0.

## 16. Metrics
| Metric | Type | Labels | Notes |
|--------|------|--------|-------|
| odgm_events_emitted_total | Counter | type=create|update|delete|summary, mode=bridge|per_entry | Increment per emitted event payload |
| odgm_event_fanout_limited_total | Counter | - | Count of batches where cap enforced |
| odgm_event_queue_len | Gauge | - | Current queue length (sampled) |
| odgm_event_queue_drops_total | Counter | reason=queue_full | Dropped before enqueue |
| odgm_event_emit_failures_total | Counter | reason=zenoh | Publish failures |
| odgm_event_dispatch_latency_ms | Histogram | mode | Time from enqueue to emit completion |
| odgm_event_batch_keys_total | Histogram | mode | Number of changed keys per batch (pre-limit) |

## 17. Configuration Defaults (Initial)
```
ODGM_EVENT_PIPELINE_BRIDGE=true
ODGM_EVENT_PIPELINE_V2=false
ODGM_MAX_BATCH_TRIGGER_FANOUT=256
ODGM_EVENT_QUEUE_BOUND=1024
ODGM_EVENT_SUMMARY_KEY_SAMPLE=32
ODGM_EVENT_COALESCE_WINDOW_MS=0
```

## 18. Phased Implementation
| Phase | Title | Deliverable |
|-------|-------|-------------|
| J0 | Bridge Summary Emitter | Object-level summary after each batch; metrics; flag gating |
| J1 | Mutation Context Capture | BatchSetValues & SetValue produce `MutationContext`; presence checks implemented |
| J2 | Per-Entry Trigger Emission | Per-entry matching + fanout cap + summary fallback |
| J3 | Coalescing (Optional) | Micro-batch window support |
| J4 | Metrics Hardening | All metrics, dashboards, alert definitions |
| J5 | Optimization | Avoid redundant presence lookups; cache last N entry existence bits |

## 19. Minimal Bridge (J0) Implementation Details
Add helper `emit_object_summary(ctx: &MutationContext)` building one `TriggerPayload` with:
```
context: {
	"changed_total": <len(changed)>,
	"sample_keys": <comma-joined first N keys>,
	"version": <version_after>
}
```
Event type: DATA_UPDATE (until a dedicated SUMMARY type is added). Always emitted even if no triggers configured (optional – configurable later; initial: always emit for tests).

## 20. Storage / Replication Adjustments Needed
- `EntryStore::batch_set_entries` & single `set_entry` augmented to: (a) collect writes, (b) pre-fetch old presence for keys in same storage transaction, (c) return a small `MutationDescriptor` with classification & versions.
- Replication path remains opaque; event capture occurs in shard layer before building request to replication.

## 21. Testing Strategy
| Test | Mode | Purpose |
|------|------|---------|
| bridge_basic_summary | J0 | Ensure summary emitted with correct counts |
| bridge_queue_drop | J0 | Fill queue; verify drop metric increments |
| per_entry_create_update_delete | J2 | Classification correctness |
| per_entry_fanout_limit | J2 | Summary fallback when over cap |
| idempotent_delete_no_event | J2 | No delete event for absent key |
| mixed_numeric_string_keys | J2 | Trigger both data_trigger & data_trigger_str |
| ordering_monotonic_versions | J2 | Events version sequence ascending |
| coalesce_window_merge | J3 | Keys merged within window (optional) |

## 22. Failure Modes & Mitigations
| Failure | Result | Mitigation |
|---------|--------|-----------|
| Queue full | Event lost | Metrics + user-configurable queue size |
| Zenoh publish fail | Event lost | Metrics + optional logging |
| Misconfigured triggers | Event skipped | Decode errors counted |
| Large batch over cap | Some events aggregated | Summary event details retained |

## 23. Migration & Rollback
- Default start in Bridge mode (no data model changes). Toggle `ODGM_EVENT_PIPELINE_V2=true` to enable per-entry emission after tests stable.
- Rollback: set `ODGM_EVENT_PIPELINE_V2=false` to revert to summary; set both flags false to disable events entirely.

## 24. Open Questions
| Question | Decision (Initial) |
|----------|--------------------|
| Should summary events be suppressed if zero triggers exist? | Emit anyway during bridge for test determinism; later conditional. |
| Distinct EventType for summary? | Defer; reuse DATA_UPDATE now. |
| Include old value hashes? | Not initially; add context key later if needed. |
| Persist queue on shutdown? | No (at-most-once). |

## 25. Future Enhancements (Not in V2 Core)
- Persistent event log (at-least-once) via append-only topic.
- Trigger condition expressions (value predicates, regex on string keys).
- Adaptive fanout scaling based on CPU/latency budgets.
- CRDT-aware semantic event classification (MERGED vs OVERWRITTEN).

## 26. Implementation Checklist (Detailed)
Legend: [x] Done, [~] In Progress / Partial, [ ] Pending, [!] Blocked

### J0 – Bridge Summary Emitter (Short-Term Stabilization)
- [x] J0.1 Bridge summary event struct & dispatcher (`BridgeSummaryEvent`, `BridgeDispatcher`)
- [x] J0.2 Bounded channel integration per shard (enqueue on single/batch mutations)
- [x] J0.3 Broadcast subscription channel (in‑process) for tests (`subscribe()`)
- [x] J0.4 Bridge enable/disable via env + precedence with V2 flag
- [x] J0.5 Auto-disable when `ODGM_EVENT_PIPELINE_V2=true` (tested)
- [~] J0.6 External publish (Zenoh topic) – deferred (currently logs + broadcast only)
- [~] J0.7 Metrics wiring: internal counters (emitted/dropped) present; exporter integration pending
- [ ] J0.8 Queue depth gauge & drop reason metrics
- [ ] J0.9 Negative/overflow tests (queue fill & drop scenario)
- [ ] J0.10 Documentation (reference.adoc) update for bridge semantics

### J1 – Mutation Context Capture & Classification
- [x] J1.1 Define `MutAction`, `ChangedKey`, `MutationContext` types
- [x] J1.2 Presence classification for create/update in `set_entry`
- [x] J1.3 Presence classification for create/update in `batch_set_entries`
- [x] J1.4 Capture accurate `version_before` for batch (updated in batch_set_entries)
- [x] J1.5 Delete path classification (single & batch delete) producing Delete actions
- [x] J1.6 Filter out idempotent deletes (absent key => no ChangedKey)
- [x] J1.7 Add unit/integration test: create→update→delete sequence (assert actions)
- [x] J1.8 Add test for mixed new & existing keys in same batch
- [x] J1.9 Edge test: empty batch (should no-op, no context)

### J2 – Per-Entry Trigger Emission
- [x] J2.1 Introduce dedicated V2 event queue (separate from bridge; unaffected by bridge disable)
- [x] J2.2 Enqueue `MutationContext` objects with sequence id
- [x] J2.3 Dispatcher task (per shard) consuming V2 queue (skeleton logs only)
- [x] J2.4 Trigger resolution: map numeric & string triggers from shard metadata / configs (inline matcher in dispatcher)
- [~] J2.5 Per-entry trigger matching & emission loop (trigger processor invoked; metrics & structured summary pending)
- [~] J2.6 Fanout accounting & early cut when cap exceeded (cap enforced; counters pending)
- [~] J2.7 Summary fallback event when fanout truncated (currently log-only)
- [ ] J2.8 Idempotent delete suppression test
- [ ] J2.9 Mixed numeric + string key trigger test
- [ ] J2.10 Ordering monotonicity test (versions ascending vs event sequence)

### J3 – Coalescing / Micro-Batching (Optional)
- [ ] J3.1 Config flag parsing (`ODGM_EVENT_COALESCE_WINDOW_MS`)
- [ ] J3.2 Dispatcher buffering within window
- [ ] J3.3 Merge algorithm (last-action-wins per key; upgrade Create→Update if seen again?)
- [ ] J3.4 Metrics: coalesced_batches_total, coalesced_keys_saved_total
- [ ] J3.5 Coalesce window disabled path benchmark (ensure negligible overhead)

### J4 – Metrics & Observability Hardening
- [ ] J4.1 Counters: emitted_total{type,mode}, queue_drops_total{reason}
- [ ] J4.2 Histograms: dispatch_latency_ms{mode}, batch_keys_total{mode}
- [ ] J4.3 Gauge: queue_len
- [ ] J4.4 Failure counter: emit_failures_total{reason}
- [ ] J4.5 Fanout limited counter: fanout_limited_total
- [ ] J4.6 Export bridge and V2 counters via observability crate
- [ ] J4.7 Dashboard spec (Grafana JSON or doc references)
- [ ] J4.8 Alert thresholds documented

### J5 – Optimization & Cleanup
- [ ] J5.1 Avoid redundant existence lookups (batch presence map micro-cache)
- [ ] J5.2 Optional small LRU (recent entry presence / action hints)
- [ ] J5.3 Fast path for single-key update (skip map alloc)
- [ ] J5.4 Reduce allocations in summary event (pre-size vector, reuse buffers)
- [ ] J5.5 Bench: compare per-entry vs bridge latency & CPU for varying k
- [ ] J5.6 Remove temporary reuse of bridge dispatcher for V2 emission (fully separate path)
- [ ] J5.7 Add dedicated EventType for summary (replace temporary DATA_UPDATE reuse)
- [ ] J5.8 Documentation polishing & deprecation notes for legacy trigger assumptions

### Cross-Cutting / Integration
- [ ] C1 Capability RPC: advertise `event_pipeline_v2=true` when V2 enabled
- [ ] C2 CLI docs update (explain bridge vs V2)
- [ ] C3 Gateway filtering (if any client-side adaptation required)
- [ ] C4 Reference doc update (topic & payload structure examples)
- [ ] C5 Add developer README snippet for enabling V2 locally

### Testing Matrix (Consolidated)
- [ ] T1 Bridge queue saturation & drop metric
- [ ] T2 Per-entry create/update/delete classification (single)
- [ ] T3 Mixed batch with existing & new keys
- [ ] T4 Batch exceeding fanout cap → summary fallback
- [ ] T5 Idempotent delete yields no event
- [ ] T6 Mixed numeric + string trigger firing
- [ ] T7 Ordering monotonic (versions never regress)
- [ ] T8 Coalescing window merges repeated keys
- [ ] T9 V2 disable fallback to bridge (bridge re-enabled scenario)
- [ ] T10 Capability flag visible to client
- [ ] T11 High churn object micro-batching performance (optional J3)

### Current Status Summary
| Area | Status | Notes |
|------|--------|-------|
| Bridge (J0) | Mostly Complete | External publish + metrics export pending |
| Mutation Context (J1) | Complete | All core paths & tests (mixed batch, empty batch) implemented |
| Per-Entry Emission (J2) | In Progress | Trigger matching active; fanout truncation logging; summary fallback log-only; metrics pending |
| Coalescing (J3) | Not Started | Deferred until base per-entry stable |
| Metrics Hardening (J4) | Not Started | Internal counters only |
| Optimization (J5) | Not Started | Will follow functional completeness |
| Docs / Capability | Not Started | Plan updated; RPC flag & docs pending |

## 27. Immediate Next Actions
1. Implement structured summary fallback emission (replace log-only) with representative keys & counts (J2.7).
2. Seed metrics: emitted_total, fanout_limited_total, queue_len gauge (partial J4 ahead of schedule).
3. Add tests: fanout limit + summary fallback (T4), idempotent delete suppression (T5), mixed numeric + string triggers (T6).
4. Ordering monotonicity test (T7) validating per-object version progression.
5. Capability RPC advertisement for `event_pipeline_v2` (C1) + README snippet (C5).
6. Legacy bridge-focused tests marked #[ignore] while advancing J2 (bridge_event_test, bridge_disable_test).

Revision History:
- 2025-10-06: Added J2 skeleton (queue, dispatcher, basic test); completed delete classification & batch version_before.
- 2025-10-06: Inline trigger matching + trigger processor instrumentation; legacy bridge tests ignored; updated J2 progress.

Upon completion of the above, mark J1 fully done and begin J2 fanout & emission work.

---
Revision History:
- 2025-10-06: Initial draft created (bridge + phased V2 design).
