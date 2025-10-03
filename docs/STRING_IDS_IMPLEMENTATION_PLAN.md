% String / Semantic Object IDs & Entry Keys – Implementation Plan & Status
% Status: In Progress (Phases 0–2 complete; Phase 3 functional pending README & gateway metrics; Phase 4 core triggers complete – docs/CLI outstanding)
% Last Updated: 2025-10-03 (post Phase 4 docs & CLI capabilities completion)

## 1. Purpose
Operational tracking document for implementing the proposals:
- `proposals/string-based-ids-and-keys.md`
- (Related) `proposals/per-entry-storage-layout.md`

This plan decomposes delivery into phases, provides an auditable checklist, defines exit criteria, metrics, owners, and rollback guidance.

## 2. Scope (This Workstream)
Included:
- Proto extensions (string object IDs, string entry keys, string data triggers)
- Normalization, validation, and configuration (`ODGM_MAX_STRING_ID_LEN`)
- Storage key encoding for string objects (null-terminated + record type layout)
- CRUD parity for string IDs (Get/Set/Merge/Delete)
- Parallel string entry map + triggers map
- Capability advertisement RPC
- Metrics & observability baseline

Deferred / Separate Workstreams:
- Granular per-entry physical storage migration
- Backfill / aliasing numeric → string IDs
- SDK adoption & client default switch
- Event payload extension for `key_str` field (may require proto evolution in EventInfo)

## 3. High-Level Phases
| Phase | Title | Goal | Rollout Mode |
|-------|-------|------|--------------|
| 0 | Schema & Guards | Add proto fields + reject usage clearly | Code merged, feature gated by returning UNIMPLEMENTED |
| 1 | Normalization & Gateway Parsing | Accept raw string form at edge (still rejected in service) | Dark path instrumentation |
| 2 | Storage Key Encoding | Persist string objects under new textual key format (write/read) | Limited / canary cluster |
| 3 | CRUD Parity | Full Set/Get/Delete/Merge for string IDs enabled | Opt‑in (env flag or feature) |
| 4 | String Entry Keys & Triggers | `entries_str` + `data_trigger_str` functional | Opt‑in then default |
| 5 | Capability Surfacing | Add Capabilities RPC & metrics dashboards | GA |
| 6 | Adoption & Metrics Drive | Client default flips to string IDs | Progressive |
| 7 | Numeric Creation Deprecation (Docs) | Warn / discourage numeric-only creation | Docs + telemetry |

## 4. Detailed Checklist (Live Status)

Legend: [ ] TODO, [~] In Progress, [x] Done, [!] Blocked / Attention

### Phase 0 – Schema & Guards (DONE)
- [x] Extend proto:
  - [x] `ObjMeta.object_id_str`
  - [x] `ObjData.entries_str`
  - [x] `ObjectEvent.data_trigger_str`
  - [x] `SingleObjectRequest.object_id_str`
  - [x] `SingleKeyRequest.key_str` & `object_id_str`
  - [x] `SetKeyRequest.key_str` & `object_id_str`
  - [x] `SetObjectRequest.object_id_str`
- [x] Regenerate prost code (implicit on build)
- [x] Update all struct initializers (gateway, cli, dev bins, tests)
- [x] Add identity module skeleton (`identity.rs`)
- [x] Service returns UNIMPLEMENTED for string variants (guard)
- [x] Add config: `ODGM_MAX_STRING_ID_LEN`
- [x] Document Phase 0 completion here

### Phase 1 – Normalization & Gateway Parsing
- [x] Normalization utilities (`normalize_object_id`, `build_identity`)
- [x] Gateway path parser: detect string vs numeric IDs & wired into routes (GET/PUT/DELETE)
  - [x] Internal enum scaffold (partially unused; kept for future refactor)
  - [x] Return 400 (Bad Request) on normalization failure
- [x] Integration test: invalid chars → 400
- [x] Integration test: over-length → 400
- [x] Integration test: GET with string ID (service enabled; prior UNIMPLEMENTED guard superseded)

### Phase 2 – Storage Key Encoding
- [x] Implement encoding helpers in `storage_key.rs`
  - [x] Skeleton functions created
  - [x] Final functions: metadata / numeric & string entry key builders
  - [x] Decoder / parser for string keys (prefix scan path placeholder)
- [x] Refactor shard code: internal API to accept `ObjectIdentity` (string or numeric)
  - [x] Placeholder shard methods for string ID get/set/delete added (later implemented)
  - [x] Implemented internal string ID CRUD (meta key only, no per-entry granularity yet)
- [x] Write path for string IDs uses new encoding (meta key persisted)
- [x] Read path resolves string meta keys via new shard methods (numeric path unaffected)
- [x] Add unit tests for key encoding (roundtrip, ordering, prefix scanning simulation)
- [x] Add micro-bench (criterion) for encoding throughput
- [x] Metrics: histogram `odgm_normalize_latency_ms` (name differs from original spec; alias decision pending)

### Phase 2a – Testing Artifacts (Sub-phase)
- [x] Unit test: numeric legacy key untouched
  - [x] String meta key shape
  - [x] Ordering meta before entries
  - [x] Parse tests (meta/numeric/string entries)
  - [x] Fuzz-ish reversible meta key (500 random ids)

### Phase 3 – CRUD Parity Enablement
- [x] Service guards removed for string IDs (gateway now passes through)
- [x] Mutual exclusivity validation
- [x] CRUD operations via `ObjectIdentity`
- [x] Response metadata injects `object_id_str`
- [x] Duplicate semantics: numeric Set=upsert; string Set=create-only (AlreadyExists on dup)
- [x] Concurrency test (race: >=1 success & >=1 AlreadyExists ensured)
- [x] String roundtrip test
- [x] Numeric & string lexical overlap test ("123" vs 123)
- [x] Both IDs set rejected (integration)
- [x] Metrics counters (set/get variant) + histogram
- [x] README update (String IDs section)

### Phase 4 – String Entry Keys & Triggers (COMPLETE)
- [x] Accept `entries_str` in Set/Merge
- [x] Serialize / store string entries (still monolithic blob)
- [x] Include `entries_str` on Get
- [x] Accept `data_trigger_str` & event manager fires based on string key (create/update/delete)
- [x] Unit + integration tests for create/update/delete triggers (string keys)
  - [x] Test: on_create fired exactly once
  - [x] Test: update vs create discrimination (existing object updated with string entry)
  - [x] Test: delete triggers on_delete (object deletion emits per-entry delete)
- [x] Metric: `odgm_entry_mutations_total{key_variant="string|numeric"}`
- [x] Doc update: trigger examples with string keys (see `STRING_ENTRY_TRIGGERS_EXAMPLES.md`)
- [x] Update `oprc-cli` (tools/oprc-cli) to surface capabilities and support string entry key operations (list/get/set with `entries_str`)
  - [x] SetStr / GetStr implemented (gRPC path)
  - [x] Tests: setstr/getstr roundtrip, duplicate create semantics
  - [x] Capabilities command added (Phase 5 linkage)
  - [x] List/Get individual string entry key subcommands
  - [x] Zenoh path support (unified numeric & string API)

### Phase 5 – Capability RPC
- [x] Add `Capabilities` RPC (+ message `CapabilitiesResponse`)
- [x] Populate booleans: `string_ids`, `string_entry_keys`, `granular_entry_storage` (future=false)
- [x] Gateway / CLI command to query capabilities
- [x] CLI tests exercising capabilities (plain + JSON)
- [x] Env flags implemented: `ODGM_ENABLE_STRING_IDS`, `ODGM_ENABLE_STRING_ENTRY_KEYS`
- [x] Integration tests verifying disabled state → UNIMPLEMENTED

### Phase 6 – Adoption Drive
- [ ] Alert if fallback (numeric) rate > threshold after target date
- [ ] CLI default create uses string IDs (random ULID / user-provided)
- [ ] Documentation: recommend string IDs for new integrations
- [ ] Set target: 70% adoption => proceed to Phase 7

### Phase 7 – Numeric Creation Deprecation (Docs)
- [ ] Emit WARN log on numeric-only create (configurable suppression)
- [ ] Update docs: numeric IDs marked “legacy”
- [ ] Provide migration guidance (alias / backfill) – link to future tool
- [ ] Prepare removal plan PR draft (not merged)

## 5. Exit Criteria per Phase
| Phase | Exit Criteria |
|-------|---------------|
| 0 | Build green; proto changes merged; guards active; doc added |
| 1 | Gateway normalization live; rejected attempts measured; no panics |
| 2 | Key encoding tests + bench passing; string object persisted & retrieved |
| 3 | All CRUD tests pass; metrics visible; duplicate semantics documented (numeric upsert, string create-only); README updated |
| 4 | Triggers fire correctly for string keys (create/update/delete) with tests |
| 5 | Capability RPC consumed by CLI; fields accurate in both modes |
| 6 | Adoption metric ≥ target for 2 consecutive weeks |
| 7 | Numeric create WARN deployed; docs updated; migration plan published |

## 6. Metrics (Initial Set)
- Counters: `odgm_object_set_total{variant}` (legacy name), `odgm_objects_created_total{variant}` (alias), `odgm_get_total{variant}`
- Counters: `odgm_entry_mutations_total{key_variant}` (Phase 4)
- Histogram: `odgm_normalize_latency_ms` (implemented; replaces spec name `odgm.object_id.normalize.ns`)
- Ratio (derived): string_objects_created / total_objects_created (Grafana)
- Gateway counters: `gateway.string_id.attempt`, `gateway.string_id.rejected`

## 7. Risks & Mitigations
| Risk | Impact | Mitigation |
|------|--------|------------|
| Key format regression / incompatibility | Data unreadable | Keep numeric path untouched until Phase 3; add tests for mixed store keys |
| Performance degradation (string normalization) | Increased p99 | Micro-bench + histogram; optimize / cache small-ID fast path |
| Duplicate ID race | Inconsistent state | Decide fail-fast semantics; do shard-level CAS before write |
| Trigger fanout explosion with many string keys | Load spike | Enforce max batch fanout limit; configuration knob |
| Partial adoption confuses clients | Support overhead | Capability RPC + docs clarity |

## 8. Rollback Strategy
- Phases 2–3: If errors detected, disable acceptance of `object_id_str` (env flag) → existing string objects remain but no new ones created.
- Keep numeric read/write path intact until numeric usage <10% + stability window passes.
- No destructive migrations in early phases; key encoding is additive.

## 9. Open Decisions
| Topic | Decision | Owner | Due |
|-------|----------|-------|-----|
| Duplicate create semantics | (TBD) Prefer AlreadyExists? | DP Team | Before Phase 3 merge |
| Gateway flag name | (TBD) `GATEWAY_ENABLE_STRING_ID_PARSE` | Gateway Maintainer | Phase 1 |
| Event payload string key field vs context map | (TBD) | Events Maintainer | Phase 4 |

## 10. Current Status Snapshot
- Phase 0: COMPLETE
- Phase 1: COMPLETE (gateway parsing + validation tests; metrics flag pending)
- Phase 2: COMPLETE
- Phase 3: FUNCTIONAL (README + gateway attempt/reject metrics pending)
- Phase 4: CORE FUNCTIONAL (string entry keys + triggers + tests + metrics); remaining: docs & CLI support
- Next Immediate Tasks:
  - Gateway counters & optional feature flag (Phase 3 leftover)
  - README update (string IDs usage & semantics) (Phase 3 leftover)
  - Docs: trigger examples with string keys (Phase 4)
  - CLI: support `entries_str` operations & capabilities surface (Phase 4)
  - Begin Capability RPC design draft (Phase 5 preparation)

## 13. Detailed Testing Checklist (Aggregated)
| Layer | Area | Test | Phase | Status |
|-------|------|------|-------|--------|
| Proto | Encoding | Backward compat (old numeric only messages deserialize) | 0 | x |
| Proto | Encoding | Unknown new fields ignored by older client (manual fixture) | 0 | x |
| Identity | Normalization | Lowercasing & allowed charset | 1 | x |
| Identity | Validation | Over-length rejected | 1 | x |
| Gateway | Routing | String ID → 501 (until Phase 3) | 1 | (superseded) |
| Gateway | Routing | Invalid chars → 400 | 1 | x |
| Gateway | Routing | Over-length → 400 | 1 | x |
| Storage | Key Format | String meta key shape | 2 | x |
| Storage | Key Format | Ordering meta < entries | 2 | x |
| Storage | Key Format | Mixed numeric & string retrieval unaffected | 2 | (implicit) |
| CRUD | String Create | Create + Get roundtrip | 3 | x |
| CRUD | Conflict | Duplicate create semantics | 3 | x |
| CRUD | Interop | Numeric & string coexist | 3 | x |
| CRUD | Validation | Both IDs set → invalid | 3 | x |
| Event | Triggers | on_create for string key | 4 | x |
| Event | Triggers | on_update vs on_create separation | 4 | x |
| Event | Triggers | on_delete for string key | 4 | x |
| Capability | RPC | Fields reflect feature enablement | 5 |  |
| Adoption | Metrics | % string IDs computed | 6 |  |
| Deprecation | Logging | WARN on numeric create | 7 |  |

Legend: x = done, blank = pending, ~ = in progress.

## 11. Contribution Guidelines (Quick)
- Keep proto changes in smallest possible PRs; accompany with regeneration note.
- Each phase PR updates this file’s checklist before merge.
- Add tests alongside functionality (no checklist item marked DONE without at least one test where applicable).

## 12. Appendix: Quick Test Matrix (Incremental)
| Case | Numeric | String | Phase |
|------|---------|--------|-------|
| Create object | ✅ | ❌ (UNIMPL) | 0–1 |
| Get object | ✅ | ❌ (UNIMPL) | 0–1 |
| Set object (merge keys) | ✅ | ❌ | 0–1 |
| Delete object | ✅ | ❌ | 0–1 |
| Create w/ both IDs set | ❌ (reject) | ❌ (reject) | 3 |
| Trigger on numeric key | ✅ | N/A | 0–3 |
| Trigger on string key | N/A | ✅ | 4 |
| Capability RPC | N/A | N/A | 5 |

---
Maintain this document as the single source of truth for implementation progress. Update “Last Updated” date on each substantive change.
