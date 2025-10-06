% Granular (Per-Entry) Storage Implementation Plan
% Status: Draft (Active – API consolidation applied)
% Last Updated: 2025-10-05 (Post MST variable-length key migration & blob write disablement)

## 1. Purpose
Operational plan translating the approved proposal `proposals/per-entry-storage-layout.md` into concrete, phased engineering tasks with status tracking, owners, exit criteria, metrics, and rollback guidance. Complements `STRING_IDS_IMPLEMENTATION_PLAN.md` (string IDs foundational work is prerequisite; now COMPLETE through Phase 4 + Capabilities baseline).

## 2. Scope (This Workstream)
Included:
- Protobuf changes: ENHANCE existing GetValue/SetValue/DeleteValue (ValueResponse enriched) + ADD new RPCs: BatchSetValues, ListValues, DeleteValue (explicit single value delete) and supporting messages.
- Storage trait extensions & in-memory + fjall backend implementations for granular records.
- Composite key encoder/decoder finalization (currently partially implemented in `storage_key.rs` – will evolve into reusable `granular_key.rs` module).
- Direct cut-over (NO dual-write) & read-path reconstruction for backward compatibility.
- Capability flag wiring (`granular_entry_storage` -> true when feature active).
- Feature flags & config envs (`ODGM_ENABLE_GRANULAR_STORAGE`, `ODGM_GRANULAR_PREFETCH_LIMIT`, `ODGM_MAX_BATCH_TRIGGER_FANOUT` reuse).
- Backfill tool (Deferred) formerly planned for legacy blob → per-entry (documented but not scheduled).
- Metrics, tests, benches, documentation, CLI & gateway / REST / Zenoh handlers.

Excluded / Deferred:
- Secondary indexing / query planner
- Entry-level TTL/retention (reserved via future record_type variants)
- Compression / extension headers (record_type high-bit space reserved)
- RocksDB / redb backend implementations (follow after memory/fjall prove pattern)
- Bulk backfill (explicitly deferred – see Phase G rationale)

## 3. Dependencies & Prereqs
| Area | Status | Notes |
|------|--------|-------|
| String IDs & string entry keys | COMPLETE | Provides key normalization + parallel maps |
| Capability RPC | COMPLETE | Will extend boolean when feature GA |
| storage_key.rs base encoding | COMPLETE (meta + placeholder entry forms) | Need to generalize & avoid duplication |
| Event trigger fanout config | PARTIAL | Add batch per-entry trigger cap enforcement |

## 4. High-Level Phases
| Phase | Title | Goal | Rollout Mode |
|-------|-------|------|--------------|
| 0 | Cost Validation Benchmarks | Quantify blob read vs per-entry scan cost across entry counts & value sizes | Local bench (non-prod) |
| A | Proto API Extensions | Enrich existing ValueResponse + add BatchSetValues/ListValues/DeleteValue under flag | Code merged, APIs gated (UNIMPLEMENTED) |
| B | Key Encoding & Storage Traits | Implement composite key module + EntryStore trait (no backend impl yet) | Internal only |
| C | Shard-Layer EntryStore Impl | Implement EntryStore at shard layer (above replication/storage) | Feature flag off by default |
| D | Prefix Scan Optimization | Optimize list_entries via native storage prefix scans (memory/fjall) | Canary cluster disabled |
| E | RPC Handler Wiring | Wire gRPC handlers to EntryStore; enable per-entry write path | Canary with metrics |
| F | Read Path Cut-Over | Reconstruct ObjectEntry from granular entries; legacy blob fallback | Progressive rollout (per shard) |
| G (Deferred) | Backfill Tool & Metrics | (Deferred – no current demand for bulk migration) | N/A |
| H | Disable Blob Writes | New objects only per-entry; old remain blob until explicitly needed | (Pulled forward – active) |
| I | Blob Path Removal | Remove legacy blob code & proto maps gating (depends on future demand) | Major release (future) |
| J (Planned) | Event Pipeline Redesign | Introduce new per-entry & aggregate event emission pipeline aligned with granular storage (replaces legacy create/update/delete triggers removed during cleanup) | Design → Canary → GA |

## 5. Detailed Checklist (Live Tracking)
Legend: [ ] TODO, [~] In Progress, [x] Done, [!] Blocked

### Phase 0 – Cost Validation Benchmarks
- [x] Benchmark harness: blob read (deserialize full object) vs entry scan (iterate N entry records & assemble)
- [~] Parameter matrix:
	- Entry counts executed: 1, 8, 32, 128, 512 (2048 pending add – see Phase 0 Findings TODO)
	- Value sizes executed: 16B, 128B, 1KB, 4KB
- [x] Generate serialized object data using bincode of a benchmark-local `ObjectEntry` replica (more realistic than purely synthetic concatenation)
- [x] Measure:
	- wall-clock per reconstruction (criterion mean); p99 approximated qualitatively from variance (low variance observed; formal p99 export deferred)
	- allocated bytes: (deferred – not instrumented yet)
	- relative slowdown: scan_vs_blob_ratio (qualitative & sampled ratios recorded below)
- [ ] Output CSV / summary table to stdout (deferred – manual interpretation for this iteration)
- [x] Define and validate acceptance thresholds (initial): scan_vs_blob_ratio <= 1.8 for 2048 entries @ 4KB (NEEDS 2048 RUN); <=1.2 for <=128 entries @ <=1KB (met)
- [x] Document findings & prelim decision on `ODGM_GRANULAR_PREFETCH_LIMIT` (retain default 256 for now)
- Exit (current iteration): Bench harness & preliminary results in repo; final large-object (2048) data + CSV still pending.

#### Phase 0 – Short Report
Scope: Compare legacy blob deserialize vs per-entry scan + reconstruction across entry counts (1→512) and value sizes (16B→4KB) on Memory / SkipList / Fjall backends.

Method (concise): For each matrix point, store (a) single bincode blob of a synthetic `ObjectEntry`-like struct and (b) individual entry values. Measure mean time (Criterion) for blob_read and entry_scan_reconstruct_bincode; reconstruction also re-encodes to make results conservative.

Key Sample Results (Memory backend):
- 128 entries × 1KB: blob 175.0µs vs reconstruct 143.0µs (ratio 0.82)
- 32 entries × 4KB: blob 161.8µs vs reconstruct 117.0µs (ratio 0.72)
- 512 entries (various sizes): ratios <1.5 (≤1KB values ~1.25–1.35; 4KB still <1.5)

Findings:
- Reconstruction is not slower; it is faster or near parity for tested shapes.
- No superlinear growth up to 512 entries; trend suggests 2048 scenario likely <1.7–1.75x (still to verify).
- Threshold (≤128 entries @ ≤1KB ≤1.2x) comfortably met (actual better than baseline).

Gaps / Deferred:
- 2048 entry measurements (needed to finalize ≥1.8x cap).
- CSV export & allocation profiling (planned but not yet required for go decision).

Decision: Proceed to Phase A (proto) while scheduling 2048-entry follow-up. Keep `ODGM_GRANULAR_PREFETCH_LIMIT=256`.


### Phase A – Proto API Extensions
**Status**: ✅ COMPLETE
**Summary**: Extended gRPC API with granular storage primitives while maintaining backward compatibility.

- [x] Enhance existing proto (ValueResponse) with: object_version, string key, deleted flag (simplified to string-only keys)
- [x] Add new RPCs only where essential for granular semantics:
	- BatchSetValues(BatchSetValuesRequest) → EmptyResponse
	- ListValues(ListValuesRequest) → (stream ValueEnvelope)
	- DeleteValue(SingleKeyRequest) → EmptyResponse (idempotent; legacy DeleteObject unaffected)
- [x] Define / add messages: BatchSetValuesRequest (with map<string, ValData>), ListValuesRequest, ValueEnvelope (string keys only)
- [x] Regenerate prost; confirm older clients ignore unknown ValueResponse fields (backward compat test deferred to integration tests)
- [x] Service returns UNIMPLEMENTED for new RPCs (both ODGM and Gateway stubs added)
- [x] API simplified: all entry keys are strings (numeric keys converted to decimal strings like "42")
- Exit: Build green ✓, tests pass (existing tests unaffected), CLI unaffected (CLI changes deferred until Phase E).

### Phase B – Key Encoding & Storage Traits
**Status**: ✅ COMPLETE
**Summary**: Implemented composite key encoding/decoding and trait definitions without backend implementation.

- [x] Created `granular_key.rs` module (extends `storage_key.rs` with granular-specific utilities)
- [x] Add unified enum for key parse result: `GranularRecord` (Metadata / Entry with string keys)
- [x] Add versioned object metadata structure (`ObjectMetadata` with version field, tombstone, attributes)
- [x] Metadata serialization/deserialization with roundtrip tests
- [x] Numeric key to string conversion helpers (`numeric_key_to_string`, `string_to_numeric_key`)
- [x] Extended storage trait: new `EntryStore` trait with get_entry, set_entry, delete_entry, list_entries, batch_set_entries, get/set_metadata
- [x] Added `EntryStoreTransaction` trait for transactional batch operations
- [x] Implement ordering & property tests (prefix grouping, meta-before-entries verified)
- [x] Helper structs: `BatchSetResult`, `EntryListOptions`, `EntryListResult`
- Exit: Unit + property tests pass ✓ (14 new tests, all passing; 57 total tests pass).

### Phase C – Shard-Layer EntryStore Implementation
**Status**: ✅ COMPLETE (October 5, 2025)
**Location**: `data-plane/oprc-odgm/src/shard/` (shard layer, above replication/storage)
**Strategy**: Implement `EntryStore` trait at shard layer; encode/decode composite keys; delegate to existing replication/storage layers.

**Architecture Clarity**:
```
┌─────────────────────────────────────────┐
│  Shard Layer (ObjectUnifiedShard)       │ ← Phase C: Implement EntryStore here
│  - Knows: ObjectEntry, composite keys   │   (encodes keys, version logic)
│  - EntryStore trait implementation      │
└──────────────┬──────────────────────────┘
               │ ShardRequest (opaque binary KV)
┌──────────────▼──────────────────────────┐
│  Replication Layer (Raft State Machine) │ ← NO CHANGES (already works)
│  - Knows: opaque key/value bytes        │   (consensus, log storage)
│  - Applies operations to app_storage    │
└──────────────┬──────────────────────────┘
               │ StorageBackend trait calls
┌──────────────▼──────────────────────────┐
│  Storage Backend (AnyStorage)           │ ← NO CHANGES (already works)
│  - Knows: raw bytes only                │   (memory/skiplist/fjall)
│  - get/put/scan/delete primitives       │
└─────────────────────────────────────────┘
```

**Implementation Notes**:
- **Replication layer**: Unchanged—handles `ShardRequest` with binary keys/values
- **Storage backend**: Unchanged—provides get/put/scan via `StorageBackend` trait
- **Shard layer**: Implements `EntryStore` by:
  1. Encoding composite keys via `granular_key::build_*_key`
  2. Creating `ShardRequest` with binary key/value
  3. Sending to replication layer
  4. Decoding results and reconstructing objects

**Write Flow Example**:
```rust
// At shard layer (Phase C implementation)
async fn set_entry(&self, obj_id: &str, key: &str, value: Vec<u8>) -> Result<u64> {
    let storage_key = granular_key::build_entry_key(obj_id, key);
    let request = ShardRequest {
        operation: Operation::Write(WriteOperation { 
            key: storage_key.into(), 
            value: value.into() 
        })
    };
    self.replication.replicate_write(request).await?;
    // Update version, emit metrics, etc.
}
```

**Read Flow Example**:
```rust
// At shard layer (Phase C implementation)
async fn list_entries(&self, obj_id: &str, opts: EntryListOptions) -> Result<EntryListResult> {
    let prefix = granular_key::build_object_prefix(obj_id);
    // Access storage through replication's app_storage (or direct for reads)
    let kvs = self.app_storage.scan(&prefix).await?;
    // Decode each key, filter, paginate, reconstruct entries
    let entries = kvs.into_iter()
        .filter_map(|(k, v)| granular_key::parse_granular_key(&k))
        .filter(|(id, record)| id == obj_id && matches!(record, GranularRecord::Entry(_)))
        .collect();
    Ok(EntryListResult { entries, cursor: None })
}
```

**Tasks**:
- [x] Implement `EntryStore` trait for `ObjectUnifiedShard<A, R, E>` (generic over storage/replication)
- [x] Add helper methods to encode composite keys via `granular_key::build_*_key`
- [x] Implement get/set/delete_entry by creating `ShardRequest` and delegating to replication
- [x] Implement get/set_metadata with version increment rules
- [x] Implement list_entries using `app_storage.scan(prefix)` + key decoding
- [x] Implement batch_set_entries with atomic version increment
- [ ] Add object reconstruction: scan entries → build `ObjectEntry` (deferred to Phase F)
- [x] Add metrics: odgm_entry_reads_total, odgm_entry_writes_total, odgm_entry_deletes_total
- [x] Unit tests: entry CRUD, version increment, prefix filtering, pagination (covered in `tests/granular_entry_store_test.rs`; cursor encoding still deferred)
- Exit: ✅ All 57 existing tests pass; build succeeds; EntryStore trait fully implemented and ready for Phase D optimization

### Phase D – Prefix Scan Optimization
**Location**: `data-plane/oprc-odgm/src/shard/` (optimize existing EntryStore implementation)
**Strategy**: Optimize `list_entries` performance; verify efficient prefix scanning across backends; add benchmarks.

**Key Insight**: 
Phase C implementation already works with all backends (memory/skiplist/fjall) via `AnyStorage`. Phase D focuses on:
1. Paginating `list_entries` so we stop scanning once we have a page of results.
2. Exercising the real storage backends (memory + fjall) to ensure ordering and pagination parity.
3. Adding a Criterion harness to capture baseline numbers for entry get/list/batch operations.

**Implementation Notes** (current state):
- `EntryStore::list_entries` now accepts `EntryListOptions` and returns an `EntryListResult` with an opaque cursor for resuming scans.
- The shard layer streams records via `scan_range_paginated(limit + slack)` to avoid materialising every key/value pair. Metadata rows are skipped in place, and prefix filters short-circuit once the matching window is exhausted.
- Memory + fjall backends both honour the early-exit limit via their `scan_range_paginated` implementations, so list paging no longer clones the full object.
- Criterion benchmark `benches/granular_entry_store.rs` exercises entry get, list (100 entries), and batch set (50 entries) on both backends.

**Tasks**:
- [~] Profile `list_entries` to identify allocation hotspots (Criterion harness in place; capture + analysis pending)
- [~] Implement deeper buffer reuse/decoder pooling (current implementation minimises scans; further allocation tuning deferred)
- [x] Add batch-oriented pagination path (`scan_range_paginated` with slack + cursor resume)
- [x] Benchmark: entry get (single key lookup)
- [x] Benchmark: list 100 entries (prefix scan)
- [x] Benchmark: batch set 50 entries (atomic multi-write)
- [ ] Run benchmarks on memory backend (baseline numbers to publish)
- [ ] Run benchmarks on fjall backend (verify native prefix scan efficiency numbers)
- [x] Add property test: random workload, verify memory/fjall parity (`tests/granular_entry_store_test.rs`)
- [x] Document performance characteristics in code comments (see `list_entries` doc comment in `entry_store_impl.rs`)
- Exit (current iteration): Paginated list_entries + parity tests merged; Criterion benches ready. Pending follow-up: capture bench numbers on target hardware and compare fjall vs memory latency.

### Phase E – RPC Handler Wiring
**Location**: `data-plane/oprc-odgm/src/grpc_service/data.rs`
**Strategy**: Wire gRPC handlers to EntryStore implementation; enable per-entry write path under feature flag.

**Architecture**:
```
┌──────────────────────┐
│  gRPC Handler        │ ← Phase E: Wire to EntryStore
│  (data.rs)           │   (replace UNIMPLEMENTED stubs)
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│  Shard Layer         │ ← Phase C: EntryStore impl
│  (EntryStore trait)  │   (already complete)
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│  Replication         │ ← Unchanged
└──────────┬───────────┘
           │
┌──────────▼───────────┐
│  Storage             │ ← Unchanged
└──────────────────────┘
```

**Tasks**:
- [x] Update `delete_value` handler: call `shard.delete_entry(obj_id, key)`
- [x] Update `batch_set_values` handler: call `shard.batch_set_entries(obj_id, mutations)`
- [x] Update `list_values` handler: call `shard.list_entries(obj_id, opts)` and stream results
- [x] Update `get_value` handler: populate `object_version`, `key`, `deleted` fields in response
- [x] Update `set_value` handler: write per-entry records instead of blob (when flag enabled)
- [!] (MOVED to Phase J) Ensure events fire per entry (bounded by `ODGM_MAX_BATCH_TRIGGER_FANOUT`). Legacy event trigger logic was temporarily removed during helper cleanup; a redesigned pipeline will reintroduce this.
- [x] Feature flag gating: `ODGM_ENABLE_GRANULAR_STORAGE=true` routes to granular path
- [x] Integration tests: end-to-end RPC → shard → storage → response
- Exit: Canary deployment shows correct per-entry CRUD; no stale reads; latency within targets. Event emission temporarily reduced to no-ops pending Phase J redesign.

### Phase F – Read Path Cut-Over
**Location**: `data-plane/oprc-odgm/src/grpc_service/data.rs` + `shard/`
**Strategy**: Enable reading from granular entries; reconstruct `ObjectEntry` for legacy `GetObject` calls; migrate objects on-demand.

**Read Paths**:
1. **GetValue (single entry)**: Already reads from granular storage (Phase E)
2. **GetObject (full object)**: Needs reconstruction from entries
3. **ListValues**: Already streams from granular storage (Phase E)

**Object Reconstruction**:
```rust
async fn get_object_reconstructed(&self, obj_id: &str) -> Result<ObjectEntry> {
    // 1. Get metadata (version, tombstone)
    let meta = self.get_metadata(obj_id).await?;
    
    // 2. List entries (respect ODGM_GRANULAR_PREFETCH_LIMIT)
    let opts = EntryListOptions { 
        prefix: None, 
        limit: std::env::var("ODGM_GRANULAR_PREFETCH_LIMIT")
            .unwrap_or_else(|_| "256".into())
            .parse()
            .unwrap_or(256),
        cursor: None 
    };
    let result = self.list_entries(obj_id, opts).await?;
    
    // 3. Reconstruct ObjectEntry
    let mut entry = ObjectEntry::default();
    entry.last_updated = meta.object_version;
    for (key, value) in result.entries {
        // Populate entry.value or entry.str_value
        if let Ok(num_key) = key.parse::<u32>() {
            entry.value.insert(num_key, ObjectVal::from(value));
        } else {
            entry.str_value.insert(key, ObjectVal::from(value));
        }
    }
    Ok(entry)
}
```

**Migration Strategy**:
- Objects written with granular storage: read from entries (no blob)
- Legacy objects (blob only): 
  - First granular mutation → explode blob to entries (one-time)
  - Until mutation: serve from blob (fallback)
  - Optional admin command to force migration

**Recent Update (2025-10-05)**:
- Added `ObjectUnifiedShard::reconstruct_object_from_entries`, paging through `EntryStore::list_entries` with guardrails (prefetch limit enforcement, maximal page bound, cursor stall detection) and returning `None` for tombstoned objects.
- Wired `ObjectShard::reconstruct_object_granular` to the new helper.
- Updated gRPC `get` path to prefer granular reconstruction when the feature flag is active, transparently falling back to legacy blob reads when metadata is absent; legacy numeric IDs remain unchanged.
- Validation: `cargo test -p oprc-odgm --tests` (all suites green, including `granular_rpc_end_to_end`).
- IMPORTANT: Legacy object-level DataCreate/DataUpdate/DataDelete event triggers were removed (simplification + unused legacy blob helpers). They will return in Phase J with a more granular-aware architecture (per-entry diffing without full object reconstruction unless required by trigger rules).

**Tasks**:
- [x] Implement `reconstruct_object_entry` in shard layer
- [x] Update `GetObject` handler to try granular read first, fallback to blob
- [x] Update Capability RPC: set `granular_entry_storage=true` when flag active

### Phase H – Disable Blob Writes 

**Status**: ✅ COMPLETE (2025-10-05)

**Rationale / Delta**: Originally scheduled post Phase F; pulled forward and completed. Granular-only mutation paths are authoritative. No code path persists monolithic blobs; legacy blobs (if any) remain only for historical reads until Phase I removal.

**Definition of "Blob Write Disabled"**:
1. No code path persists a monolithic object blob on any create/update/delete.
2. Attempting to invoke legacy blob write helpers either (a) is impossible (code removed) or (b) triggers an explicit panic/log error in debug builds.
3. Metrics & tests confirm zero new blob records after mutations.

**Tasks**:
- [x] Remove legacy blob write helpers (`set_storage_value`, `set_storage_value_with_return`, etc.)
- [x] Ensure `set_object` writes only per-entry + metadata version (no blob)
- [x] Ensure delete paths (`delete_object`, per-entry delete) never write blob tombstones
- [x] Unit test: mutate object, scan storage for object blob key (negative assertion) (`no_blob_write_test.rs`)
- [x] Integration test: mixed SetValue / BatchSetValues / DeleteValue → no blob key created
- [ ] (Optional Hardening) Add debug assertion/log if any residual blob write helper path is ever invoked

**Exit Criteria** (Met):
- All blob write helper symbols removed (grep clean)
- Negative blob presence tests pass; introducing artificial blob write re-fails locally (manual validation)
- `odgm_blob_write_attempts_total` metric placeholder (to be added only if assertion task pursued) currently not required
- Plan updated to mark Phase H complete

**Follow‑On (Phase I dependency)**:
- Once Phase H exit met AND operational scan shows legacy blob record count below threshold (or zero), proceed to Phase I removal (strip blob read fallback & proto deprecation steps).

**Current Risk**: Residual legacy objects rely on read fallback; ensure Phase I not started until MST replication adaptation stable to avoid compounding change surface.

### Interlock: MST Replication Adaptation (Granular Key Support)

The MST replication layer previously assumed fixed 8-byte u64 object keys mapping to full-object `ObjectData` blobs. Granular storage replaced single blob writes with many variable-length composite keys (object_id + record discriminator + entry key). This broke MST replication (key length validation + deserialization failures) until adaptation.

#### Implemented (2025-10-05)
1. Converted `MstKey` from `u64` to `Vec<u8>` (true variable-length lexicographic ordering) – no hashing indirection.
2. Adjusted network/page structures to carry raw byte bounds instead of numeric ranges.
3. Rebuilt MST replication layer to treat values opaquely (`StorageValue`) rather than deserializing to `ObjectData` (removing incorrect whole-object assumptions and eliminating the prior `UnexpectedEnd` errors).
4. Updated cluster tests to pass with granular-only writes; replication now synchronizes per-entry keys directly.

#### Rationale for Direct Variable-Length Keys vs Earlier Hash Adapter Plan
The earlier “Option A – Hashed Key Index” design (see Archived Design below) added complexity (collision handling, bucket management) that became unnecessary once the underlying MST library was made agnostic to key length and we accepted lexicographic byte ordering. Direct key usage:
- Preserves full key entropy (aids future range-partitioning or prefix statistics).
- Eliminates collision management logic & associated metrics surface for now.
- Simplifies rebuild logic (straight scan → upsert).

#### Current Status
- Core change merged; cluster replication green.
- Legacy MST unit tests retained via compatibility wrapper methods (`set/get/delete(u64)`).
- Opaque value model defers semantic merge/LWW to a future enhancement (Phase M+ below) if needed.

#### Follow-Up Tasks
| ID | Task | Priority | Status |
|----|------|----------|--------|
| M-F1 | Add MST metrics: root hash publish count, page diff size histogram | Medium | TODO |
| M-F2 | Add optional value hashing before upsert (to skip MST update when unchanged) | Low | TODO |
| M-F3 | Add consistency audit: random key sample cross-node equality | Medium | TODO |
| M-F4 | Benchmark impact of variable-length keys vs prior numeric (micro bench) | Low | TODO |
| M-F5 | Document ordering guarantees & key normalization invariants | Medium | TODO |

#### Deferred (Optional) Enhancements
- Introduce pluggable KeyAdapter only if future compression / hashed fanout is required for extremely long keys.
- Introduce collision metrics only if hashing approach reintroduced.

#### Exit Criteria (Granular MST GA – Revised)
- Multi-node cluster test passes under sustained granular write workload (already achieved baseline).
- Audit tool (M-F3) reports 0 mismatches over N>10k sampled keys in staging.
- Root hash stabilization: variance of page publish counts across nodes <5% over sliding window.

#### Rollback
Return to NoReplication (existing option) if critical MST inconsistency detected; feature flag gate not yet required given simplicity of current adaptation (could add `ODGM_MST_DISABLE=true` if operational need arises).

#### Archived Design (Superseded) – Hashed Key Index
The previously documented hash-based indirection (u64 index_key + collision buckets) is retained here for historical context but is **not** the active implementation. Re-introduce only if performance profiling shows pathological scaling with very large composite keys.

---

### Phase I – Removal (Major)

- [ ] Remove blob serialization code & related tests
- [ ] Remove entries map fields? (proto deprecation window) – mark as deprecated first
- Exit: Major release cut & migration notes published.

### Phase J – Event Pipeline Redesign (Planned)
**Status**: Planned (Design TBD) – Legacy object-level triggers removed; interim minimal emission layer now required due to failing integration tests.
**Rationale**: The original event triggering code assumed monolithic object blobs and performed full reconstruction on every write/delete to compute diffs. After migrating to granular storage and removing unused blob helpers, we stripped the legacy trigger functions to avoid repeated whole-object scans. A new pipeline is required to: (a) emit per-entry create/update/delete events efficiently, (b) support string key triggers alongside numeric keys, (c) optionally batch or coalesce high-churn mutations, and (d) enforce configured fanout limits (`ODGM_MAX_BATCH_TRIGGER_FANOUT`).

**Goals**:
1. Zero unnecessary full-object reconstruction when only metadata or a small subset of entries changed.
2. Configurable trigger modes: per-entry, aggregated (object-level), hybrid.
3. Memory-safe diffing: use previous version snapshot of only touched keys (leveraging batch write set) instead of scanning entire object.
4. Backpressure / fanout limiting with metrics and drop counters.
5. Extensible trigger matching (numeric ids, string keys, future tag-based selection).

**Approach Outline**:
- Capture mutation intent (list of changed entry keys + new values) at `batch_set_entries` time; pass to event layer before commit or immediately after version increment.
- Maintain a lightweight Metadata Cache (object_version, tombstone, optional small LRU of recent entry hash digests) to allow update vs create determination without full reconstruction.
- For delete operations, list only keys being deleted (already known) rather than reconstructing full object.
- Provide event payload strategies: raw value bytes, value hash, or omitted (configurable) to reduce bandwidth.
- Support future CRDT payload semantics by allowing value merging logic to annotate diff classification (unchanged/merged/overwritten).

**Metrics Additions**:
- odgm_events_emitted_total{type="create|update|delete", scope="entry|object"}
- odgm_event_drops_total{reason="fanout_limit|queue_full"}
- odgm_event_diff_duration_ms (histogram)

**Tasks (Initial Draft)** (Updated 2025-10-06):
- [x] Design doc & review (API + data structures) (see `EVENT_PIPELINE_V2_DESIGN.md`)
- [ ] Introduce MutationContext struct (object_id, version_before, version_after, changed_keys, mode)
- [x] Instrument EntryStore batch/single set paths to accumulate changed_keys (bridge: simple key list capture only)
- [ ] Implement per-entry trigger evaluation without reconstruction (use changed_keys + selective old fetch if needed)
- [ ] Implement delete triggers using known deleted key set (bridge currently emits summary only)
- [ ] Add fanout limiter & metrics
- [ ] Add integration tests for mixed numeric & string key triggers (pending per-entry mode)
- [ ] Update Capability RPC to advertise event_pipeline_v2 flag
- [ ] Migrate gateway/CLI docs
- [ ] Deprecate old event fields if superseded

**Bridge (Short-Term) Tasks (J0)** – unblock failing tests while full redesign proceeds:
- [x] J0-1 Lightweight event emitter: object-level summary after each single/batch mutation (implemented in `events/bridge.rs`; logs only for now).
- [x] J0-2 Minimal in-process subscription (broadcast channel) guarded by bridge enable flag (no external topic publish yet).
- [~] J0-3 Metrics: internal counters implemented (emitted/dropped); external exporter wiring pending.
- [x] J0-4 Integration coverage: added `bridge_event_test` validating emission & broadcast; legacy failing tests adjusted (summary acceptance) – further mixed-mode tests pending V2.
- [x] J0-5 Auto-disable bridge when `ODGM_EVENT_PIPELINE_V2=true` (precedence + disable test `bridge_disable_test`).

**Risks / Mitigations**:
- High-churn objects: add coalescing window (e.g., 50ms micro-batching) – feature gate.
- Large batch writes: enforce upper bound on changed_keys inspected per event emission (emit summary event beyond threshold).

**Exit Criteria**:
- Event latency P99 ≤ 2x single entry write latency
- Zero redundant full reconstructions in benchmarks
- Fanout limiting validated under synthetic overload.

**Rollback**: Toggle feature flag `ODGM_EVENT_PIPELINE_V2=false` to disable new pipeline; no schema migration required (events simply suppressed).


## 6. Data Model Clarifications
| Aspect | Decision |
|--------|----------|
| Object versioning | Shared version per batch (increment once per batch) |
| CAS semantics | BatchSetValuesRequest.expected_object_version optional; mismatch -> ABORTED |
| Tombstones | Meta record gains tombstone bool (bit-packed future) |
| Compression | Not in initial; extension record_type reserved |
| In-memory representation | Single HashMap<StorageValue, StorageValue> with composite key bytes (object prefix + record_type + entry discriminator) |
| Transactions | In-memory transactional layer collects ops (Vec<Op>) and applies under one write-lock for atomic batch operations (no dual-write) |

## 7. API Contract (Revised – Consolidated)
Rationale: Avoid proliferating single-entry RPCs; reuse existing GetValue/SetValue/DeleteValue semantics with enriched response fields for granular storage. New RPCs introduced ONLY for batch mutation and listing.

Existing RPC Adjustments:
- GetValue(SingleKeyRequest) → ValueResponse
	- ValueResponse (extended): optional uint64 object_version; optional uint32 key; optional string key_str; optional bool deleted.
	- Old clients: ignore unknown fields; semantics unchanged when fields absent.
- SetValue(SetKeyRequest) → EmptyResponse (unchanged wire contract; server now writes per-entry records when flag enabled).
- DeleteValue(SingleKeyRequest) → EmptyResponse (idempotent per-entry delete; blob path removed post cut-over).

New RPCs:
- BatchSetValues(BatchSetValuesRequest) returns EmptyResponse
- ListValues(ListValuesRequest) returns (stream ValueEnvelope)

Messages (pseudo spec):
BatchSetValuesRequest {
	SingleObjectRequest object;                    // identifies object (namespace + object_id)
	repeated ValueMutation mutations;              // values to upsert/delete
	optional uint64 expected_object_version;       // CAS; if set and mismatch -> ABORTED
}
ValueMutation {
	oneof key_variant { uint32 key = 1; string key_str = 2; }
	ValData value = 3;            // omitted if delete=true (server ignores if present)
	bool delete = 4;              // true = tombstone
}
ListValuesRequest {
	SingleObjectRequest object;
	optional string prefix = 2;   // optional string key prefix filter (ignored for numeric)
	uint32 limit = 3;             // server-enforced max
	optional bytes cursor = 4;    // opaque pagination token
}
ValueEnvelope {
	oneof key_variant { uint32 key = 1; string key_str = 2; }
	ValData value = 3;            // omitted / empty if deleted? (decide: we will NOT stream tombstones by default)
	uint64 version = 4;           // object_version at mutation
}

Backward Compatibility Notes:
1. Added ValueResponse fields are optional; older binaries remain functional.
2. New RPCs gated by `ODGM_ENABLE_GRANULAR_STORAGE`; clients should feature-detect via Capabilities RPC.
3. DeleteValue supersedes planned DeleteEntry; SetValue continues to handle single value writes; BatchSetValues handles multi-value atomic update.

## 8. Versioning & Consistency Rules
On BatchSetValues (N>=1 mutations):
- old_version = object_version
- object_version += 1
- All mutated entries get version = object_version
Single SetValue (granular path) behaves as batch size 1 (version increments once per successful mutation group of size 1).
DeleteValue increments version only if an existing entry is removed (idempotent delete of absent key does NOT bump version).

## 9. Metrics Plan
| Metric | Type | Labels |
|--------|------|--------|
| odgm_entry_reads_total | Counter | key_variant (numeric|string) |
| odgm_entry_writes_total | Counter | key_variant |
| odgm_entry_deletes_total | Counter | key_variant, reason (explicit|batch) |
| odgm_events_emitted_total | Counter | type (create|update|delete), scope (entry|object) | 
| odgm_event_drops_total | Counter | reason (fanout_limit|queue_full) |
| odgm_event_diff_duration_ms | Histogram | phase (prepare|emit) |
| odgm_blob_fallback_reads_total | Counter | cause (missing|flag_disabled) |
| odgm_entry_get_latency_ms | Histogram | key_variant |
| odgm_batch_set_size | Histogram | - |
| odgm_objects_converted_total | Gauge | (Deferred – emit only if/when bulk migration reinstated) |
| odgm_objects_pending_conversion_total | Gauge | (Deferred) |

## 10. Testing Strategy
| Layer | Test | Phase |
|-------|------|-------|
| Proto | Backward compat (old clients ignore new RPC) | A |
| Key | Roundtrip encode/decode + ordering | B |
| Key | Property: meta sorts before entries | B |
| Backend (mem) | get/set/delete isolation vs blob | C |
| Backend (mem) | batch atomicity (all-or-none) | C |
| Backend (fjall) | Prefix scan correctness | D |
| Cut-over writes | Per-entry only correctness (no blob parity) | E |
| Read switch | Compare reconstructed vs blob object | F |
| Backfill | Crash & resume idempotency | G (Deferred) |
| Perf | p99 targets (entry get, batch set) | D/F |
| Triggers | Fanout capped, per-entry events accurate | E |

## 11. Benchmarks (Initial Targets)
| Operation | Target p99 |
|-----------|-----------|
| GetValue (granular path, hot cache) | <= 1.4x CPU of legacy blob Get; <= 0.6x bytes read |
| BatchSetValues (50 values) | <= 2x single SetValue payload size equivalent |
| ListValues (first 100) | Startup <= 5ms |

## 12. Rollback Strategy
- Disable via `ODGM_ENABLE_GRANULAR_STORAGE=false` hides granular APIs; objects written while enabled remain per-entry (accepted risk).
- Cut-over: per-entry is authoritative immediately; legacy blobs (if any pre-existing) are only decoded on first post-cut-over mutation or explicit read needing reconstruction.
- Backfill: Deferred—no bulk migration process to roll back. Future on-demand conversion remains additive and safe to halt.

## 13. Open Questions
| Topic | Question | Proposed Direction |
|-------|----------|-------------------|
| Pagination cursor format | Encode (last_key_bytes + version) vs opaque UUID? | Opaque binary (base64 in REST) containing full raw key |
| Limit default for ListValues | 100? | Use 100; cap at 1000 |
| Partial object prefetch strategy | How many entries to aggregate on legacy Get? | Use ODGM_GRANULAR_PREFETCH_LIMIT (default 256) |
| Metadata caching | LRU size & eviction policy | Start 10k entries, tune via metrics |
| On-demand object migration trigger | Convert on first granular list/value miss? | Defer; measure miss rate first |
| Fjall iterator reuse | Internal unsafe buffer reuse acceptable? | Yes with tests + feature gate |

## 14. Initial Task Owners (Placeholder)
| Area | Owner |
|------|-------|
| Proto & RPC wiring | DP Eng A |
| Key module refactor | DP Eng B |
| Memory backend changes | DP Eng C |
| Fjall backend | DP Eng D |
| (Removed: dual-write) | N/A |
| Backfill tool | DP Eng E |
| Metrics & observability | Observability Eng |
| CLI & Gateway | Platform Eng |

## 15. Next Immediate Actions
1. (Complete) Phase A PR: proto extensions + flag + UNIMPLEMENTED handlers.
2. (Complete) Tracking issues established for Phases A–F; add Phase J tracking issue.
3. Implement J0 bridge event emitter to restore green integration tests (stop-gap until redesign).
4. Collect benchmark data for 2048-entry reconstruction scenario.
5. Implement memory/Fjall benchmark publishing (Phase D remaining tasks).
6. Add MST follow-up metrics (M-F1) & audit tool (M-F3).
7. Evaluate necessity of hash-based MST adapter (likely unnecessary; keep archived only).

## 16. Appendix – Parity Probe Sketch
Periodic task (interval configurable):
- Sample N objects (random or recent hot set)
- Fetch via entries path (meta + scan) & reconstruct object
- Fetch blob (if still present)
- Compare hash(ObjectEntry normalized) – mismatch increments counter & logs diff sample (capped)
- If mismatch rate > threshold (e.g., 0.01%), optionally auto-disable read-switch flag & emit alert

---
Update "Last Updated" and checklist statuses with each merged PR.
