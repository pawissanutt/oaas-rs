% Granular (Per-Entry) Storage Implementation Plan
% Status: Draft (Initial Plan)
% Last Updated: 2025-10-03

## 1. Purpose
Operational plan translating the approved proposal `proposals/per-entry-storage-layout.md` into concrete, phased engineering tasks with status tracking, owners, exit criteria, metrics, and rollback guidance. Complements `STRING_IDS_IMPLEMENTATION_PLAN.md` (string IDs foundational work is prerequisite; now COMPLETE through Phase 4 + Capabilities baseline).

## 2. Scope (This Workstream)
Included:
- Protobuf additions (GetEntry, SetEntry, DeleteEntry, BatchSetEntries, ListEntries + new messages)
- Storage trait extensions & in-memory + fjall backend implementations for granular records
- Composite key encoder/decoder finalization (currently partially implemented in `storage_key.rs` – will evolve into reusable `granular_key.rs` module)
- Dual-write (blob + per-entry) & phased read-path switchover
- Capability flag wiring (`granular_entry_storage` -> true when APIs active)
- Feature flags & config envs (`ODGM_ENABLE_GRANULAR_STORAGE`, `ODGM_GRANULAR_PREFETCH_LIMIT`, `ODGM_MAX_BATCH_TRIGGER_FANOUT` reuse)
- Backfill tool (streaming + crash-safe) for legacy blob → per-entry
- Metrics, tests, benches, documentation, CLI & gateway / REST / Zenoh handlers

Excluded / Deferred:
- Secondary indexing / query planner
- Entry-level TTL/retention (reserved via future record_type variants)
- Compression / extension headers (record_type high-bit space reserved)
- RocksDB / redb backend implementations (follow after memory/fjall prove pattern)

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
| A | Flag Scaffolding & Proto | Add RPCs/messages under disabled flag | Code merged, APIs gated (UNIMPLEMENTED) |
| B | Key Module & Trait Ext | Implement composite key module + storage trait extensions (no service wiring) | Internal only |
| C | Memory Backend + Unit Tests | Granular record R/W for memory backend (no dual-write) | Feature flag off by default |
| D | Fjall Backend + Prefix Scan | Efficient prefix iteration & benchmarks | Canary cluster disabled |
| E | Cut-Over Activation | Per-entry only writes (legacy blob disabled) | Canary with metrics |
| F | Blobless Read Validation | Reconstruct from entries; capability advertises true | Progressive rollout (per shard) |
| G (Deferred) | Backfill Tool & Metrics | (Deferred – no current demand for bulk migration) | N/A |
| H | Disable Blob Writes (New) | New objects only per-entry; old remain blob until explicitly needed | Config flip |
| I | Blob Path Removal | Remove legacy blob code & proto maps gating (depends on future demand) | Major release (future) |

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


### Phase A – Flag Scaffolding & Proto
- [ ] Add RPCs to `oprc-data.proto`: GetEntry, SetEntry, DeleteEntry, BatchSetEntries, ListEntries
- [ ] Add messages: BatchSetEntriesRequest, EntryMutation, ListEntriesRequest, EntryEnvelope
- [ ] Regenerate prost; update `CapabilitiesResponse` if needed (already has field)
- [ ] Service returns UNIMPLEMENTED when `ODGM_ENABLE_GRANULAR_STORAGE=false`
- Exit: Build green, tests pass, CLI unaffected.

### Phase B – Key Module & Trait Extensions
- [ ] Extract / rename `storage_key.rs` -> `granular_key.rs` (re-export old names for compatibility)
- [ ] Add unified enum for key parse result (Meta / NumericEntry / StringEntry) – (DONE in current file; adjust naming)
- [ ] Add versioned object metadata structure (version field) persisted separately
- [ ] Extend object storage trait (new sub-trait `EntryStore`): get_entry, set_entry, delete_entry, list_entries, batch_set_entries
- [ ] Implement ordering & property tests (prefix grouping, meta-before-entries, no collisions w/ NUL terminator)
- Exit: Unit + property tests pass.

### Phase C – Memory Backend
- [ ] Replace current monolithic HashMap with a SINGLE map keyed by composite encoded bytes (decision: single map chosen for consistency & backend parity). Rationale: mirrors disk backends (prefix/prefix+range scans), reduces duplication, simplifies transaction atomicity (one structure), avoids double lookups & extra indirection.
- [ ] Implement entry R/W operations & maintain object_version increment rules
- [ ] Implement aggregator to reconstruct legacy object view (respect `ODGM_GRANULAR_PREFETCH_LIMIT`)
- [ ] Add metrics: odgm_entry_reads_total, odgm_entry_writes_total, odgm_entry_deletes_total
- [ ] Add histogram: odgm_entry_get_latency_ms
- Exit: All unit tests green; micro-bench shows acceptable overhead (<10% vs baseline get/set for small objects)

### Phase D – Fjall Backend
- [ ] Implement prefix scan using `string_object_prefix` (shared composite key module)
- [ ] Optimize iterator to avoid allocations (reuse buffers)
- [ ] Benchmarks: entry get, list 100 entries, batch set 50 entries
- [ ] Add property test ensuring parity w/ memory backend for random workload
- Exit: Bench p99 targets within spec (preliminary)

### Phase E – Cut-Over Activation (No Dual-Write)
- [ ] Update Set/Merge paths: persist ONLY per-entry records + metadata (legacy blob write disabled under flag)
- [ ] BatchSetEntries: single Raft log entry for all mutations + version bump
- [ ] Ensure events fire per entry (bounded by `ODGM_MAX_BATCH_TRIGGER_FANOUT`)
- [ ] Metrics: odgm_blob_reconstruction_reads_total (legacy blob decode count), odgm_entry_writes_total
- [ ] Feature flag gating: `ODGM_ENABLE_GRANULAR_STORAGE=true` enables new RPCs + per-entry write path
- Exit: Canary shows correct per-entry CRUD; reconstruction of untouched legacy objects successful; no stale read anomalies.

### Phase F – Blobless Read Validation
- [ ] Eliminate blob dependency for any object mutated after Phase E
- [ ] Pre cut-over objects: first mutation triggers decode & explode blob -> entries (one-time); optional admin command to force migrate
- [ ] Capability RPC sets granular_entry_storage=true when flag active
- [ ] Internal probe validates reconstructed object invariants (version monotonicity, key uniqueness)
- Exit: >95% reads served from entries only; reconstruction overhead within target.

### Phase G – Backfill Tool (Deferred)
// Decision: There is currently no product / operational demand for proactive bulk migration of legacy blob-only objects to per-entry physical layout.
// Rationale:
// 1. Cut-over path (Phase E) writes only granular representation (no dual-write).
// 2. Read path (Phase F) reconstructs object view from entries; legacy blob fallback used only until first post-cut-over mutation.
// 3. Avoids operational cost, extra tooling surface, and metrics noise until clear adoption driver emerges (e.g., >X% objects frequently accessed via GetEntry/ListEntries).
// 4. Lazy / on-demand conversion (future option) can be introduced by migrating an object at first granular access miss.
// Action Items (NOT scheduled):
//  - If demand surfaces, resurrect original Phase G spec from git history.
//  - Potential lightweight alternative: background sampler converting top-K hot legacy objects.
// Exit (for future reinstatement): Same as original (idempotent crash-safe run) plus SLA that lazy fallback latency stays within budget.

### Phase H – Disable Blob Writes (New)
- [ ] Config to stop writing blob for new / updated objects
- [ ] Warn log if blob path invoked while disabled
- Exit: Blob write rate <1% (legacy only) for 2 weeks.

### Phase I – Removal (Major)
- [ ] Remove blob serialization code & related tests
- [ ] Remove entries map fields? (proto deprecation window) – mark as deprecated first
- Exit: Major release cut & migration notes published.

## 6. Data Model Clarifications
| Aspect | Decision |
|--------|----------|
| Object versioning | Shared version per batch (increment once per batch) |
| CAS semantics | BatchSetEntriesRequest.expected_object_version optional; mismatch -> ABORTED |
| Tombstones | Meta record gains tombstone bool (bit-packed future) |
| Compression | Not in initial; extension record_type reserved |
| In-memory representation | Single HashMap<StorageValue, StorageValue> with composite key bytes (object prefix + record_type + entry discriminator) |
| Transactions | In-memory transactional layer collects ops (Vec<Op>) and applies under one write-lock for atomic batch operations (no dual-write) |

## 7. API Contract (Draft)
Brief (exact field numbers to assign after collision check):
- rpc GetEntry(SingleKeyRequest) returns ValueResponse
- rpc SetEntry(SetKeyRequest) returns EmptyResponse
- rpc DeleteEntry(SingleKeyRequest) returns EmptyResponse
- rpc BatchSetEntries(BatchSetEntriesRequest) returns EmptyResponse
- rpc ListEntries(ListEntriesRequest) returns (stream EntryEnvelope)

Messages (pseudo):
BatchSetEntriesRequest { SingleObjectRequest object; repeated EntryMutation mutations; optional uint64 expected_object_version; }
EntryMutation { oneof key_variant { uint32 key = 1; string key_str = 2; } ValData value = 3; bool delete = 4; }
ListEntriesRequest { SingleObjectRequest object; optional string prefix = 2; uint32 limit = 3; optional bytes cursor = 4; }
EntryEnvelope { oneof key_variant { uint32 key = 1; string key_str = 2; } ValData value = 3; uint64 version = 4; }

## 8. Versioning & Consistency Rules
On BatchSetEntries (N>=1 mutations):
- old_version = object_version
- object_version += 1
- All mutated entries get version = object_version
Single SetEntry behaves as batch size 1.
DeleteEntry increments version only if an existing entry is removed.

## 9. Metrics Plan
| Metric | Type | Labels |
|--------|------|--------|
| odgm_entry_reads_total | Counter | key_variant (numeric|string) |
| odgm_entry_writes_total | Counter | key_variant |
| odgm_entry_deletes_total | Counter | key_variant, reason (explicit|batch) |
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
| GetEntry (hot cache) | <= 1.4x CPU of full Get; <= 0.6x bytes read |
| BatchSet (50 entries) | <= 2x single SetObject payload size equivalent |
| ListEntries (100) | Startup <= 5ms |

## 12. Rollback Strategy
- Disable via `ODGM_ENABLE_GRANULAR_STORAGE=false` hides granular APIs; objects written while enabled remain per-entry (accepted risk).
- Cut-over: per-entry is authoritative immediately; legacy blobs (if any pre-existing) are only decoded on first post-cut-over mutation or explicit read needing reconstruction.
- Backfill: Deferred—no bulk migration process to roll back. Future on-demand conversion remains additive and safe to halt.

## 13. Open Questions
| Topic | Question | Proposed Direction |
|-------|----------|-------------------|
| Pagination cursor format | Encode (last_key_bytes + version) vs opaque UUID? | Opaque binary (base64 in REST) containing full raw key |
| Limit default for ListEntries | 100? | Use 100; cap at 1000 |
| Partial object prefetch strategy | How many entries to aggregate on legacy Get? | Use ODGM_GRANULAR_PREFETCH_LIMIT (default 256) |
| Metadata caching | LRU size & eviction policy | Start 10k entries, tune via metrics |
| On-demand object migration trigger | Convert on first GetEntry miss? | Defer; measure miss rate first |
| Fjall iterator reuse | Internal unsafe buffer reuse acceptable? | Yes with tests + feature gate |

## 14. Initial Task Owners (Placeholder)
| Area | Owner |
|------|-------|
| Proto & RPC wiring | DP Eng A |
| Key module refactor | DP Eng B |
| Memory backend changes | DP Eng C |
| Fjall backend | DP Eng D |
| Dual-write integration | DP Eng A + C |
| Backfill tool | DP Eng E |
| Metrics & observability | Observability Eng |
| CLI & Gateway | Platform Eng |

## 15. Next Immediate Actions
1. Phase A PR: proto extensions + flag + UNIMPLEMENTED handlers.
2. Create tracking issues for each Phase in Git (link from this doc).
3. Begin key module refactor concurrently (low coupling) – ensure minimal churn for later merge.

## 16. Appendix – Parity Probe Sketch
Periodic task (interval configurable):
- Sample N objects (random or recent hot set)
- Fetch via entries path (meta + scan) & reconstruct object
- Fetch blob (if still present)
- Compare hash(ObjectEntry normalized) – mismatch increments counter & logs diff sample (capped)
- If mismatch rate > threshold (e.g., 0.01%), optionally auto-disable read-switch flag & emit alert

---
Update "Last Updated" and checklist statuses with each merged PR.
