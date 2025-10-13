% String-Based Object IDs & Entry Keys for ODGM
% Status: Draft
% Owner: Data Plane / ODGM Maintainers
% Last Updated: 2025-10-02

## 1. Problem Statement

The ODGM (Object Data Grid / Manager) currently models:

- Object identity: `uint64 object_id`
- Entry (field) keys inside an object: `map<uint32, ValData> entries`

While simple and space‑efficient, this integer‑only addressing creates growing limitations:

| Area | Limitation Today | Impact |
|------|------------------|--------|
| External integration | Upstream systems naturally use order IDs, UUIDs, ULIDs, email/usernames, semantic keys | Requires fragile translation layers & lookup indirection |
| Multi‑tenant / logical scoping | Embedding tenant or namespace context requires extra metadata lookups | Higher latency & complexity |
| Schema evolution | Adding new “virtual collections” via key prefixing is awkward with numeric IDs | Slows iteration |
| Observability & debugging | Integer IDs lack human meaning in logs & traces | Operator burden |
| Event routing / triggers | Fine‑grained triggers keyed by semantic field names need mapping to integers | Complexity & risk |
| Client ergonomics | SDKs must allocate and track numeric IDs server‑side | Higher coupling |

Goal: Introduce *native string-based identifiers* for both object IDs and per-object entry keys, while preserving backwards compatibility and performance characteristics (deterministic partition routing, Raft log determinism) **without adding a stored hash field**.

## 2. High-Level Objectives

1. Allow creating & addressing objects directly by a client-supplied string ID.
2. Allow object entries to be keyed by descriptive string names (e.g. `profile.email`, `cart.items`, `cfg:v1`).
3. Maintain existing integer IDs & numeric entry keys during a transition window (dual mode).
4. Avoid breaking existing gRPC / Zenoh / REST consumers until migration flag flips.
5. Preserve sharding, replication and Raft log determinism via canonical hashing of string IDs.
6. Keep storage memory overhead minimal and lookups O(log n) or O(1) depending on backend.
7. Provide clear migration tooling & observability for adoption progress.

## 3. Non-Goals

- Not redesigning overall ODGM consistency or replication.
- Not introducing full secondary indexing (future extension).
- Not removing numeric IDs immediately; deprecation spans ≥ 2 minor releases.
- Not changing trigger semantics beyond allowing string keys.

## 4. Stakeholders & Consumers

- Control Plane (CRM) templates & injection logic.
- Gateway REST API (object addressing & field operations).
- Invocation subsystem & trigger dispatcher.
- Storage backends: in-memory, `fjall`, `skiplist`, potential RocksDB (future).
- Observability / tracing (ID surface in spans & logs).

## 5. Requirements

### Functional
1. Create / PUT object using either numeric or string ID (mutually exclusive per object).
2. GET / DELETE / MERGE must accept either form.
3. Entry mutation & retrieval (future: field‑level GET) supports string keys.
4. Event triggers (`ObjectEvent`) may reference either numeric key (`uint32`) or string key.
5. SDKs can query capability: server advertises `string_ids_supported=true`.
6. REST & Zenoh paths accept string segments (URL & Zenoh key safe).

### Non-Functional
1. Hash-based shard selection overhead for string IDs ≤ +15% vs numeric baseline in p99.
2. Memory overhead per string ID stored ≤ 48 bytes avg (excluding user-provided string payload) with interning / arena or copy-on-write strategies.
3. Migration path zero-downtime under rolling upgrade of cluster.
4. Deterministic hashing: identical string always maps to same shard across replicas (stable algorithm & normalization).

## 6. Design Overview

Introduce *Versioned Identity Envelope* concept:

```
ObjectIdentity =
	enum Variant { NUMERIC=0, STRING=1 }
	variant: Variant
	// Exactly one populated:
	uint64 object_id (existing)
	string object_id_str (new)
```

Similarly for entries:

```
EntryKey =
	enum Variant { NUMERIC=0, STRING=1 }
	uint32 key (existing)
	string key_str (new)
```

Rather than explode `ObjData.entries` into a dual-key map (unsupported directly in proto), we add a new parallel map: `map<string, ValData> entries_str = <next_id>;` while keeping `map<uint32, ValData> entries = 2;`.

### Partitioning (No Stored Hash)

Partition ID remains an explicit argument (`partition_id`) chosen by clients / control plane logic. The introduction of string IDs does **not** change partition routing semantics in this proposal. We deliberately avoid persisting any hash / fingerprint:

Reasons to omit stored hash:
- Simpler metadata schema (no extra optional field).
- Removes need for collision buckets / synthetic numeric surrogate IDs.
- Direct string keys already supported by storage backends (see companion per-entry storage proposal).
- Hash can still be computed transiently where needed (e.g., optional lock striping, metrics sampling) without committing to a stable on-disk representation.

Future extension: If we introduce auto-partitioning (derived from ID) we can add a stateless mapping `partition = fast_hash(normalized_id) % N` without persisting the hash output. This keeps today’s design lean while preserving extensibility.

### Normalization Rules
- Lowercase ASCII only enforced? (Option A) or general Unicode NFC (Option B). Proposed: *Option A + explicit UTF-8 allowed but normalized to NFC then lowercased for ASCII subset; reject control characters.*
- Maximum length: 160 bytes (fits under common index / path limits). Configurable via env `ODGM_MAX_STRING_ID_LEN`.
- Allowed characters for REST/ZENOH path without encoding: `[a-z0-9._:-]`. Others must be percent-encoded client-side; server normalizes after decoding.

### Storage Representation (Updated – Null‑Terminated ID + Record Type)

We align with the granular per‑entry storage proposal (Option A) to maximize consistency and enable efficient ordered scans by object ID.

Key formats (base object store keys inside a partition/collection):

```
Metadata key: <object_id_utf8><0x00><0x00>
Entry key (numeric): <object_id_utf8><0x00><0x10><u32_be>
Entry key (string):  <object_id_utf8><0x00><0x11><key_len_varint><key_utf8>
```

Rules & rationale:
- `object_id_utf8` is the normalized ID (string form). Numeric object IDs are encoded as un‑padded decimal ASCII for a unified path (legacy 8‑byte binary form remains readable but is not used for *new* string ID creation).
- A single NUL (0x00) terminates the object ID; normalization forbids embedded NUL ensuring unambiguous parsing.
- `record_type` byte (0x00 metadata, 0x10 numeric entry key, 0x11 string entry key) keeps metadata adjacent to entries while sorting first.
- String entry keys use a varint length to keep overhead low for short names.

Lookup paths:
1. Normalize object ID if string; for numeric path convert to decimal ASCII string if we opt to expose a string alias (phase‑dependent).  
2. Build key as above; perform point GET (metadata) or prefix scan `<object_id><0x00>` filtering out `record_type == 0x00` for entries.

Legacy numeric consideration:
- Existing purely numeric objects stored under 8‑byte big‑endian keys remain addressable through a *compatibility branch* until backfill/migration. New writes with string ID feature enabled use the unified textual form.
- A background (optional) rewrite can re‑encode 8‑byte keys into `<decimal_ascii><0x00><0x00>` if uniformity / range ordering over mixed legacy + new objects becomes operationally valuable.

Extensibility:
- Record types beyond 0x11 remain available (0x01–0x0F, 0x12–0x7F). High‑bit space (0x80–0xFF) reserved for future flag-extended encodings (compression, TTL metadata, durability hints).

Rejected (reiterated):
- Length‑prefix first: prevents natural lexical ordering grouped by length.
- Dual index (length + secondary range index): unnecessary complexity now.

Optional micro‑optimizations: ephemeral hash only for lock striping / sampling (never persisted); small SSO / arena for short IDs.

### Event Triggers

Extend `DataTrigger` to reference either numeric or string entry keys. We introduce `map<string, DataTrigger> data_trigger_str = <new_id>;` parallel to existing `map<uint32, DataTrigger> data_trigger = 2;`.

Invocation events referencing object IDs remain unaffected; logging surfaces prefer `object_id_str` if present else numeric.

## 7. API & Schema Changes

### Protobuf (`oprc-data.proto`) Additions

Add fields (numbers illustrative; must avoid collisions with existing numbers):

```
message ObjMeta {
	string cls_id = 1;
	uint32 partition_id = 2;
	uint64 object_id = 3;               // existing numeric
	optional string object_id_str = 4;   // new
}

message ObjData {
	optional ObjMeta metadata = 1;
	map<uint32, ValData> entries = 2;            // existing
	map<string, ValData> entries_str = 4;         // new (3 taken by event)
	optional ObjectEvent event = 3;               // existing
}

message ObjectEvent {
	map<string, FuncTrigger> func_trigger = 1;
	map<uint32, DataTrigger> data_trigger = 2;
	map<string, DataTrigger> data_trigger_str = 3;  // new
}

message SingleObjectRequest {
	string cls_id = 1;
	uint32 partition_id = 2;
	uint64 object_id = 3;              // old
	optional string object_id_str = 4; // new
}

message SingleKeyRequest {
	string cls_id = 1;
	uint32 partition_id = 2;
	uint64 object_id = 3;
	uint32 key = 4;
	optional string object_id_str = 5; // new
	optional string key_str = 6;       // new
}

message SetKeyRequest { ... add key_str + object_id_str }
message SetObjectRequest { ... add object_id_str }
```

Binary compatibility: Existing fields unmodified; new fields are optional and only interpreted if set. Older servers ignore them; older clients ignore unknown fields (Proto3). This supports rolling upgrade.

### REST Path Extensions

Support both forms simultaneously using heuristic:

`/api/class/<class>/<partition>/objects/<id-or-token>`

Parsing logic:
1. If segment matches strictly `[0-9]+` and fits in `u64`, treat as numeric.
2. Else treat as string ID (after URL decoding & normalization).

Field (entry) endpoints (future additions):
`/api/class/<class>/<partition>/objects/<id>/values/<field>` where `<field>` uses same disambiguation: numeric regex vs string key.

### Zenoh Key Space

Maintain existing numeric patterns; introduce string ID variant with distinct separator to avoid ambiguity with trailing invocation identifiers:

Numeric: `oprc/<cls>/<partition>/objects/<u64>`

String:  `oprc/<cls>/<partition>/sobjects/<string>` (introduce `sobjects` namespace) – avoids parsing ambiguity & simplifies subscription filters. Similarly for invokes & results:

`oprc/<cls>/<partition>/sobjects/<id>/invokes/<fn>`

We keep numeric namespace unchanged; clients opt into string variant.

### Feature Flags / Capabilities

Environment variables in ODGM:

| Env | Default | Description |
|-----|---------|-------------|
| `ODGM_ENABLE_STRING_IDS` | `false` | Enables accepting & storing string object IDs |
| `ODGM_ENABLE_STRING_KEYS` | `false` | Enables `entries_str` updates |
| `ODGM_STRING_IDS_WRITE_ONLY` | `false` | Accept writes but hide from listings (staged rollout) |
| `ODGM_MAX_STRING_ID_LEN` | `160` | Length constraint |

Capability surface (gRPC Health / CrmInfo): advertise `string_ids=true`, `string_keys=true`.

## 8. Internal Module Changes

| Module | Change |
|--------|--------|
| `oprc-odgm` object store | Add string ID path: normalize + encode `<id_utf8><0x00><type>`; legacy numeric 8‑byte read support; new numeric alias uses decimal ASCII |
| Storage traits (`oprc-dp-storage`) | No change (byte‑oriented); provide encoder/decoder utilities in ODGM layer |
| Raft log entries | Canonical identity: variant byte + (string_len varint + UTF‑8) OR variant + 8‑byte BE (legacy) / decimal ASCII (new) |
| Invocation routing | Extend `Routable` / requests to optionally carry string ID |
| Metrics | New counters: `objects.string.created`, `entries.string.set`, histogram `id.normalize.ns` |
| Tracing | Span fields: `object.id_str`, fallback to numeric if absent |

## 9. Migration Strategy (Phased)

| Phase | Description | Exit Criteria |
|-------|-------------|---------------|
| 0 | Implement schema + dual parsing behind flags | All unit tests green |
| 1 | Enable read support cluster-wide (`ODGM_ENABLE_STRING_IDS=true`, writes still blocked) | No errors in canary logs |
| 2 | Enable write for canary clients (`WRITE_ONLY=true`) | Monitor collision / perf metrics <= budget |
| 3 | Full write + read; client SDKs default to string IDs | 95% new objects string-based |
| 4 | Deprecate numeric in docs; emit warning on numeric create | <10% numeric creations |
| 5 | Remove numeric creation path (still readable) | 0 critical clients depend |
| 6 | (Optional) Remove numeric fields in major version | Major release cut |

### Data Backfill (If Required)
Numeric objects can be *aliased* with a generated canonical string (e.g., base32 of numeric) for uniform client consumption. Provide an asynchronous job to materialize `object_id_str` for old objects (populating `ObjMeta.object_id_str` but retaining numeric ID as authoritative until Phase 5).

## 10. Observability & Tooling

- New metric: `odgm_string_id_normalize_errors_total`.
- Histogram: `odgm_string_id_length`.
- Log sampling: on collision bucket insertion (WARN with involved IDs, hash64).
- Admin debug endpoint: `/debug/object-hash/<cls>/<partition>/<id_str>` returns computed hash & shard.

## 11. Performance Considerations

Benchmark plan (Criterion benches in `oprc-odgm/benches`):

| Benchmark | Description |
|-----------|-------------|
| `normalize_string_id` | Measure normalization + length‑prefix encoding on varied lengths |
| `lookup_string_object` | String ID encode → storage get path |
| `mixed_id_workload` | 50/50 numeric & string operations microbench |
| `raft_log_serialize` | Variant + length vs numeric encoding cost |

Target budgets (p99 vs numeric baseline):

- Normalize+hash ≤ 250ns for IDs ≤ 32 bytes.
- Additional memory per active string ID entry (excluding string itself) ≤ 16 bytes.

Mitigations if exceeded:
1. Intern frequently used IDs in a lock-free string interner.
2. Switch to SIMD-optimized hashing (e.g., HighwayHash) if xxhash64 insufficient.

## 12. Backwards Compatibility & Risks

| Risk | Mitigation |
|------|------------|
| Client mis-parses ID vs key path segments | Separate `sobjects/` namespace; explicit documentation |
| Hash collision | Removed (no persisted hash; direct key uses full string) |
| Legacy numeric (8‑byte) vs new textual form divergence | Transitional decoder supports both; optional rewrite tool; metrics on legacy key access rate |
| Increased memory fragmentation | Use arena / bump allocator for stored string copies or small-string optimization container |
| Mixed-mode complexity causing bugs | Comprehensive unit + property tests (proptest) for dual identities |
| Accidental acceptance of malformed IDs | Strict regex + normalization error paths w/ metrics |
| Raft determinism impacted by non-canonical serialization | Canonical encode: variant + varint(len)+UTF‑8 (string) OR variant+8BE (legacy numeric); fuzz determinism |

## 13. Testing Strategy

1. Unit tests: normalization, acceptance/rejection, hash stability across process.
2. Property tests: `id -> hash64` stable; collisions produce bucket with both retrievable.
3. Integration tests: mixed clients performing CRUD; triggers referencing string keys fire correctly.
4. Upgrade tests: Cluster with old version (numeric) → rolling upgrade enabling flags → ensure no lost ops.
5. Fuzz: Raft log decode with random variant ordering refuses invalid forms.

## 14. Rollout Playbook (Operational)

1. Deploy code with flags off (Phase 0). Observe no regressions.
2. Turn on read support only. Confirm capabilities endpoint updates. SDK gating logic remains off for writes.
3. Canary enable writes; watch metrics for abnormal latencies or error spikes.
4. Expand to full fleet. Start emitting adoption metrics dashboards.
5. After adoption target reached, schedule deprecation announcement & set timeline.

## 15. Open Questions

| Topic | Question | Proposed Direction |
|-------|----------|-------------------|
| Unicode policy | Fully allow Unicode? | Allow but normalize; discourage visually confusable chars via lint later |
| Shard stability across future hash impl updates | How to change hash? | Versioned hash field; keep old until rebalancing job runs |
| Listing / scan APIs | Will we expose object listing with prefix? | Future extension once string IDs widely used |
| Trigger key precedence | If both numeric & string map refer to same logical field? | Treat as separate; no implicit linkage |

## 16. Implementation Tasks (Engineering Checklist)

- [ ] Protobuf field additions + regenerate code (`oprc-grpc`).
- [ ] Add normalization + hashing utility (shared crate? `oprc-models` or new `oprc-id`).
- [ ] Extend storage trait for dual identity struct.
// Removed: synthetic hash index & collision handling (direct string keys)
- [ ] REST router: dual path parsing + `sobjects` namespace.
- [ ] Zenoh key handlers for `sobjects`.
- [ ] Feature flag wiring (`envconfig`).
- [ ] Metrics & tracing field additions.
- [ ] Backfill job (optional early) incl. optional generation of `object_id_str` aliases for numeric.
- [ ] Benchmarks & property tests.
- [ ] Docs & SDK updates.
- [ ] (Interlock) Reuse normalization/hash functions for per-entry storage (see `per-entry-storage-layout.md`).
- [ ] (Interlock) Ensure composite key encoding reserves variant bits for ID types (shared with granular design).

## 17. Versioning & Release Strategy

- Additions ship in minor release (e.g., `v0.x+1`).
- Deprecation notices added to CHANGELOG once adoption > 80%.
- Numeric removal only in next *breaking* (major / pre-1.0: coordinate with first `0.y` → `0.y+1` semver policy or `1.0`).

## 18. Appendix

### A. Normalization Pseudocode (No Stored Hash)

```rust
pub fn normalize_id(id: &str) -> String {
    // 1. NFC normalize (unicode-normalization crate)
    let nfc = nfc_normalize(id);
    // 2. Lowercase only ASCII A-Z
    lowercase_ascii(&nfc)
}
```

### B. Example Proto Diff (Excerpt)

```
message SingleKeyRequest {
	string cls_id = 1;
	uint32 partition_id = 2;
	uint64 object_id = 3;
	uint32 key = 4;
	optional string object_id_str = 5;
	optional string key_str = 6;
}
```

### C. Example REST Calls

```
# Create object with string ID
PUT /api/class/order/0/objects/ord_2025_09_12345

# Set string field key
PUT /api/class/order/0/objects/ord_2025_09_12345/values/status (future)
Body: { "value": "shipped" }
```

---

Feedback welcome. After sign-off, proceed with Phase 0 tasks & open tracking issues. See companion proposal `per-entry-storage-layout.md` for granular storage changes that *share* only the normalization utility (no stored hash).

