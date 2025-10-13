% Per-Entry (Granular) Object Storage Layout for ODGM
% Status: Draft
% Owner: Data Plane / ODGM Maintainers
% Last Updated: 2025-10-02

## 1. Context & Motivation

Today each object persists a single serialized blob (map<uint32, ValData>) representing all its entries. This coarse granularity creates several limitations:

| Area | Limitation | Impact |
|------|------------|--------|
| Partial reads | Fetching one hot field requires deserializing entire object | Latency & CPU waste on large sparse objects |
| Hot / cold separation | All fields share same lifecycle & caching behavior | Inefficient memory & IO usage |
| Schema evolution | Adding/removing fields rewrites whole blob | Amplified write cost |
| Field-level triggers | Detecting precise changes requires diffing full map | Extra CPU; limits trigger richness |
| Indexing | Building secondary indexes requires scanning full objects | Slower index maintenance |
| TTL / retention | Cannot expire individual fields independently | Reduced flexibility |

Goal: Store each entry independently under a composite key, enabling fine‑grained IO, selective retrieval, partial replication optimizations, and future indexing.

This proposal is designed to complement the parallel migration to string-based object IDs & entry keys (see `string-based-ids-and-keys.md`).

## 2. High-Level Objectives

1. Persist each (object, entryKey) pair as a distinct record.
2. Preserve atomic semantics for multi-entry updates where required (batch API).
3. Maintain backward compatibility: existing GET object returns aggregated view assembled on demand.
4. Allow *incremental* migration of objects from monolithic to granular representation.
5. Expose optional field-level APIs (GetEntry, SetEntry, DeleteEntry, ListEntries).
6. Support both numeric and string object IDs / entry keys transparently.

## 3. Non-Goals

- Not introducing full global secondary indexing yet (will leverage new layout later).
- Not changing Raft consensus scope (still replicates logical ops, not low-level storage mechanics).
- Not removing existing blob format until after deprecation window.

## 4. Data Model

### 4.1 Logical

Logical Object = Metadata + Set<Entry>

```
Entry {
  key: EntryKeyVariant (numeric u32 OR string)
  value: ValData
  version: u64               // per-entry MVCC sequence (monotonic per object)
  last_updated: u64 (ts)     // optional optimization
}
```

Object Metadata retains:
- Identity (numeric or string + hash64)
- Global object version (incremented on any batch applying >0 entry mutations)
- Event configuration (ObjectEvent)
- Optional tombstone flag

### 4.2 Physical Key Encoding (Updated – Option A: Null‑Terminated Object ID + Record Type)

Goal: Preserve natural lexicographic ordering over object IDs (enabling efficient range scans) while supporting fast per‑object prefix enumeration of entries and future extensibility.

Key design principles:
1. Put the raw normalized object ID bytes first (no length prefix) so range scans over `[id_start, id_end]` are direct.
2. Use a single terminator byte `0x00` (which normalization forbids inside IDs) to delimit the object ID.
3. Follow with a one‑byte `record_type` distinguishing metadata vs entry variants (and reserving space for future flags/compression bits).
4. Encode entry key variant inline (no extra separator needed) after `record_type`.

#### 4.2.1 Key Forms

```
Metadata record key: <object_id_utf8><0x00><0x00>
Entry (numeric key): <object_id_utf8><0x00><0x10><u32_be>
Entry (string key):  <object_id_utf8><0x00><0x11><key_len_varint><key_utf8>
```

Where:
- `object_id_utf8`: normalized UTF‑8 (string IDs as provided; numeric IDs encoded as decimal ASCII – see 4.2.3).
- Terminator `0x00`: unambiguous boundary for parsing.
- `record_type` byte ranges (proposed initial allocation):
  - 0x00 = metadata
  - 0x10 = entry (numeric key)
  - 0x11 = entry (string key)
  - 0x8x = reserved for future flags (high bit indicates extension / compressed / TTL etc.)
- `u32_be`: big‑endian 4‑byte numeric entry key.
- `key_len_varint`: compact length (LEB128 / varint) of string entry key for minimal overhead on short keys.

Rationale:
- No dependency on object ID length for ordering.
- Metadata key sorts immediately before all its entries because 0x00 < 0x10/0x11.
- Single pass parse: scan until NUL, read `record_type`, branch decode.
- Easily extended: additional record classes (e.g. secondary index pointer, tombstone marker) can occupy unused codes (0x01–0x0F, 0x12–0x1F, etc.).

#### 4.2.2 Prefix / Enumeration Semantics

To enumerate all entries for an object:
1. Build base prefix: `P = <object_id><0x00>`.
2. Perform a range/prefix scan over keys with that prefix.
3. Filter out the single metadata record (`record_type == 0x00`), processing only `0x10` / `0x11` for entries.

Metadata point lookup:
```
K_meta = <object_id><0x00><0x00>
```
Direct GET avoids scanning entries.

Optional micro‑optimization: If backend supports “exact prefix with minimum suffix”, issuing two range bounds `[K_meta, K_meta]` fetches only metadata.

#### 4.2.3 Numeric Object ID Representation (Option A Chosen)

Numeric IDs are rendered as *un-padded* decimal ASCII (e.g. `"12345"`).

Pros of un-padded decimal:
- Simpler implementation; no fixed width decision.
- Smaller for small numbers.

Trade‑off: Lexicographic order is *not* numeric order beyond pure decimal string ordering (e.g. `"100"` sorts before `"20"`). Acceptable because strategic direction is toward string IDs; numeric range scans are legacy/low frequency. If strict numeric ordering becomes important, we can introduce zero‑padded width (20 digits) in a future migration with an alternate record_type or collection‑local rewrite.

#### 4.2.4 Entry Key Variant Encoding

Entry keys follow immediately after `record_type`:
- Numeric: 4 bytes big‑endian → supports direct binary compare & constant size.
- String: varint length + UTF‑8 (normalized). Varint chosen to avoid fixed 2‑byte overhead on very short keys; length ≤ 255 likely dominates typical case.

#### 4.2.5 Full Object Reconstruction

Rehydrate object:
1. GET `<object_id><0x00><0x00>` for metadata (version, events, tombstone, etc.).
2. Scan prefix `<object_id><0x00>` collecting all `record_type` in {0x10,0x11}.
3. Aggregate into logical maps (numeric vs string entries).

Optimizations (unchanged conceptually): bloom / negative cache; early stop if client requested limited subset (future field paging API).

#### 4.2.6 Object ID Collision & Prefix Safety

- Two distinct object IDs produce distinct byte sequences before the first `0x00`; one being a textual prefix of another does *not* cause ambiguity because the shorter ID’s metadata key terminates earlier (`...<0x00><0x00>`), while the longer ID has additional non‑zero bytes before its own terminator.
- No need for a length prefix; termination scanning is O(len_id) which is bounded (≤ 160 bytes default cap).

#### 4.2.7 Future Flags & Extensions

Reserve high‑bit (0x80) variants of `record_type` for extended headers:
- Example: 0x90 = entry (string key, compressed), followed by compression header.
- Example: 0xA0 = metadata (tombstone marker only) – lightweight GC artifact.

Parsing rule: if `record_type & 0x80 != 0`, read an extension header varint describing additional attributes before entry key payload.

#### 4.2.8 Removal of Hash & Collision Bucket (Reaffirmed)

No persisted hash or collision bucket is required. Partition / shard selection may still hash the *normalized* ID transiently, but the physical key is the full ID. Duplicate creation attempts are logically rejected prior to storage commit.

Comparison vs previous length‑prefixed approach:
| Aspect | New (NUL + type) | Old (length prefix) |
|--------|------------------|---------------------|
| Range scan by ID | Native, ordered by ID bytes | Groups by length first |
| Per-object enumeration | Prefix `<id>\0` | Prefix `<len><id>` |
| Parse cost | Scan to NUL (≤160) | Read u16 then trust length |
| Memory overhead | +2 bytes meta, +2+ entry | +2 bytes both |
| Extensibility | Direct via record_type | Needed more variant bytes |

### 4.3 Collision Strategy (Object ID Uniqueness)

With null‑terminated IDs and explicit record_type namespace, physical key collisions only occur if two clients submit identical normalized IDs. No extra mechanism required. Hashing remains purely a routing concern (not stored).

Metadata value payload (unchanged conceptually):
```
ObjectMetaRecord {
  object_id_str?: String
  numeric_id?: u64
  object_version: u64
  event_config?: ObjectEvent
  tombstone: bool
  attributes: Map<String,String>
}
```

## 5. API Extensions (gRPC / REST / Zenoh)

### 5.1 gRPC Additions (DataService)

```
rpc GetEntry (SingleKeyRequest) returns (ValueResponse);
rpc SetEntry (SetKeyRequest) returns (EmptyResponse);
rpc DeleteEntry (SingleKeyRequest) returns (EmptyResponse);
rpc BatchSetEntries (BatchSetEntriesRequest) returns (EmptyResponse);
rpc ListEntries (ListEntriesRequest) returns (stream EntryEnvelope);
```

New messages:
```
message BatchSetEntriesRequest {
  SingleObjectRequest object = 1;          // object identity (supports string)
  repeated EntryMutation mutations = 2;    // must be non-empty
  optional uint64 expected_object_version = 3; // CAS (optimistic concurrency)
}

message EntryMutation {
  oneof key_variant {
    uint32 key = 1;
    string key_str = 2;
  }
  ValData value = 3;
  bool delete = 4; // if true, value ignored
}

message ListEntriesRequest {
  SingleObjectRequest object = 1;
  optional string key_prefix = 2;  // applies to string keys only
  uint32 page_size = 3;            // server may cap
  optional bytes cursor = 4;       // opaque pagination
}

message EntryEnvelope {
  oneof key_variant {
    uint32 key = 1;
    string key_str = 2;
  }
  ValData value = 3;
  uint64 version = 4;
}
```

### 5.2 REST

```
GET    /api/class/<cls>/<part>/objects/<id>/values/<field>
PUT    /api/class/<cls>/<part>/objects/<id>/values/<field>
DELETE /api/class/<cls>/<part>/objects/<id>/values/<field>
GET    /api/class/<cls>/<part>/objects/<id>/values?prefix=foo&cursor=...
POST   /api/class/<cls>/<part>/objects/<id>/values/batch
```

`<field>` disambiguation (numeric vs string) as in the string ID proposal.

### 5.3 Zenoh

Introduce namespaced granular entry topics:

```
GET  oprc/<cls>/<part>/objects/<obj>/values/<field>          // sync fetch
PUT  oprc/<cls>/<part>/objects/<obj>/values/<field>          // async set
PUT  oprc/<cls>/<part>/objects/<obj>/values-batch/<invk_id>  // async batch
SUB  oprc/<cls>/<part>/objects/<obj>/values/<field>/change   // change events

String object IDs: use `sobjects` namespace as defined earlier.
```

## 6. Consistency & Atomicity

Single entry updates are atomic by virtue of underlying key-value write. Batch operation encodes all mutations in a single Raft log entry. Application of log entry on followers writes entries sequentially but under a guaranteed ordering; either all succeed or node panics (crash -> replay ensures idempotency because version assignment is deterministic at apply time via (object_version + i)).

Versioning rules:

```
object_version starts at 0.
On BatchSetEntries applying N>=1 mutations:
  old_version = object_version
  object_version += 1
  each entry.version = object_version (shared) OR (old_version+1) uniform; choose shared for simplicity.
```

We opt for *shared version per batch* for simpler CAS semantics.

## 7. Event & Trigger Integration

Existing triggers referencing numeric or string keys now fire directly without diffing whole object:

- On SetEntry: classify event: create vs update based on existence of prior entry.
- On DeleteEntry: delete event.
- BatchSetEntries: emit per-entry events (bounded by config `ODGM_MAX_BATCH_TRIGGER_FANOUT`, default 256) to avoid unbounded amplification.

## 8. Migration Strategy

| Phase | Action | Notes |
|-------|--------|-------|
| A | Ship code behind `ODGM_ENABLE_GRANULAR_STORAGE=false` | No functional change |
| B | Enable dual-write (write blob + per-entry) in canary | Validate perf delta (implicit; no dedicated flag) |
| C | Flip read-path to prefer per-entry; fallback to blob if missing | Metrics gating |
| D | Stop writing blob for new objects; keep for legacy ones | Track adoption metric |
| E | Offline / streaming job converts remaining blobs to entries | Idempotent; safe restart |
| F | Remove blob deserialization path (major release) | After threshold <1% |

### 8.1 Dual-Write Details

On object update (SetObjectRequest / Merge):
1. Parse blob entries.
2. Write each entry record.
3. Persist metadata (object_version+1).
4. Original blob persisted only during Phases A–C implicitly; dropped for new writes starting Phase D (no explicit feature flag).

### 8.2 Backfill Tool

`odgm-backfill --mode=entries --collection=<c> --partition=<p> [--parallel=32]`:
- Scans blob objects lacking per-entry marker.
- Decomposes & writes entries transactionally via RAFT (or directly if offline single-node maintenance mode).

## 9. Storage Backend Adjustments

| Backend | Change |
|---------|--------|
| Memory  | Replace nested map with: HashMap<ObjectKey, ObjectMetaCache> + HashMap<EntryCompositeKey, ValData> |
| Fjall   | Key prefix layering using composite binary key; iterator for prefix scans per object |
| Skiplist | Similar to memory keyed on composite struct implementing Ord |

Cache: maintain small LRU of fully materialized objects for legacy APIs (size limited; invalidated via version bump events).

## 10. Performance Targets

| Operation | Target p99 after warm cache |
|-----------|-----------------------------|
| GetEntry (string object + string key) | ≤ 1.4x baseline Get(full object) CPU but ≤ 0.6x bytes read |
| BatchSet 50 entries | ≤ 2.0x single SetObject of equivalent payload |
| ListEntries (100 entries) | Stream startup ≤ 5ms |

Key iteration must avoid heap allocations per entry beyond ValData (reuse key decode buffer where possible).

## 11. Feature Flags

| Env | Default | Description |
|-----|---------|-------------|
| `ODGM_ENABLE_GRANULAR_STORAGE` | false | Master switch enabling per-entry storage APIs |
| `ODGM_GRANULAR_PREFETCH_LIMIT` | 256   | Max entries aggregated when constructing full object |
| `ODGM_MAX_BATCH_TRIGGER_FANOUT` | 256   | Trigger emission cap per batch |

## 12. Metrics & Observability

New counters:
- `odgm_entry_reads_total`
- `odgm_entry_writes_total`
- `odgm_entry_deletes_total`
- `odgm_blob_fallback_reads_total`

Histograms:
- `odgm_entry_get_latency_ms`
- `odgm_batch_set_size`

Gauges:
- `odgm_objects_converted_total`
- `odgm_objects_pending_conversion_total`

## 13. Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Increased number of physical writes for multi-field updates | Provide batch API; encourage clients to group writes |
| Fragmentation in storage backend | Periodic compaction / merge operations |
| Higher metadata lookup cost (per entry) | Metadata cache keyed by object hash64 | 
| Trigger storm from large batch | Fanout cap + optional coalesced summary event |
| Backfill interfering with live traffic | Throttle & yield; track backlog metric |

## 14. Testing Strategy

1. Unit: composite key encoding/decoding roundtrip.
2. Property: ordering guarantee of composite key preserves prefix grouping.
3. Integration: dual-write correctness; selective disable ensures blob read fallback works.
4. Load test: heavy GetEntry vs legacy Get to confirm IO reduction for sparse access workloads.
5. Backfill simulation: random dataset, crash mid-run, restart, ensure idempotency.

## 15. Interplay with String ID Proposal

Per-entry storage leverages the same canonical hashing & normalization for object ID and optionally for string entry keys. Hash of entry key is *not* required for primary addressing but may later accelerate secondary indexes; keep normalization utility generic.

## 16. Implementation Checklist

- [ ] Protobuf additions (new RPCs & messages) under guarded feature flag.
- [ ] Composite key module (`granular_key.rs`).
- [ ] Storage trait extension: `get_entry`, `set_entry`, `delete_entry`, `list_entries`, `batch_set`.
- [ ] Memory backend implementation.
- [ ] Fjall backend implementation (iterator + prefix support).
- [ ] Dual-write adapter layer.
- [ ] Backfill tool binary (optional separate crate or feature).
- [ ] Metrics registration.
- [ ] REST & Zenoh handlers.
- [ ] Tests & benches.
- [ ] Documentation & SDK surfaces.

## 17. Open Questions

| Topic | Question | Direction |
|-------|----------|-----------|
| Entry-level TTL | Needed now? | Postpone; design pluggable retention meta |
| Compression | Per-entry vs object-level? | Add optional value compression flag later |
| Batch atomicity across objects | Scope? | Out-of-scope (object boundary remains atomic unit) |

---

Feedback appreciated. Coordinates with string-based ID rollout; either can proceed first but shared normalization utility should be implemented once.
