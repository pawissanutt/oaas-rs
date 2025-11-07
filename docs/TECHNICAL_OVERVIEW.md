# ODGM Technical Overview

**Version:** 1.0  
**Date:** November 2025  
**Status:** Current Implementation

This document provides a technical overview of three major architectural features in the ODGM (Object Data Grid Manager) system: String-based Object Identifiers, Granular Per-Entry Storage, and V2 Event Pipeline.

---

## 1. String-Based Object Identifiers

### Overview
ODGM supports both numeric (u64) and string-based object identifiers. String IDs enable human-readable object references and align with modern distributed system patterns.

### Design

#### Identity Types
```rust
pub enum ObjectIdentity {
    Numeric(u64),
    Str(String),  // normalized form
}
```

**Normalization Rules:**
- ASCII lowercase conversion
- Character set: `[a-z0-9._:-]+`
- Maximum length: configurable (default 160 chars)
- UTF-8 encoding

#### API Contract
```rust
// Build identity from gRPC request
pub fn build_identity(
    numeric: Option<u64>,
    string_id: Option<&str>,
    max_len: usize,
) -> Result<ObjectIdentity, NormalizationError>
```

**Mutually exclusive**: Requests must provide exactly one of `object_id` (numeric) or `object_id_str` (string).

### Storage Layout

String IDs use a composite key structure with null-byte delimiters:

```
Format: <object_id_utf8><0x00><record_type>

Record Types:
  0x00 - Metadata record
  0x01 - Event configuration
  0x02 - View blob (deprecated with granular storage)
  0x10 - Numeric entry (u32 big-endian)
  0x11 - String entry (varint length + UTF-8)
```

**Example Keys:**
```
"user-123"<0x00><0x00>           → metadata
"user-123"<0x00><0x10><0x00002A> → numeric entry key=42
"user-123"<0x00><0x11><0x04>name → string entry key="name"
```

### Network Protocol

**Zenoh Topic Pattern:**
```
oprc/<class>/<partition>/objects/<object_id>
```

For string IDs, the normalized form is used directly in the key expression. Zenoh SET operations work for both identifier types; GET operations currently redirect string ID requests to gRPC.

### Compatibility

- **Numeric IDs**: Legacy 8-byte big-endian keys remain unchanged
- **String IDs**: New composite key format with record type discrimination
- **Migration**: Both types coexist; no data migration required

---

## 2. Granular Per-Entry Storage

### Overview
Granular storage replaces object-blob serialization with fine-grained per-entry persistence. Each object field is stored independently with atomic update semantics.

### Architecture

#### Core Trait
```rust
#[async_trait]
pub trait EntryStore {
    async fn get_metadata(&self, id: &str) -> Result<Option<ObjectMetadata>>;
    async fn set_metadata(&self, id: &str, meta: ObjectMetadata) -> Result<()>;
    
    async fn get_entry(&self, id: &str, key: &str) -> Result<Option<ObjectVal>>;
    async fn set_entry(&self, id: &str, key: &str, val: ObjectVal) -> Result<()>;
    async fn delete_entry(&self, id: &str, key: &str) -> Result<()>;
    
    async fn batch_set_entries(
        &self,
        id: &str,
        values: HashMap<String, ObjectVal>,
        expected_version: Option<u64>,
    ) -> Result<u64>;
    
    async fn list_entries(
        &self,
        id: &str,
        options: EntryListOptions,
    ) -> Result<EntryListResult>;
}
```

### Data Model

#### Metadata Structure
```rust
pub struct ObjectMetadata {
    pub object_version: u64,  // monotonic counter
    pub tombstone: bool,       // soft delete marker
    pub attributes: Vec<(String, String)>,
}
```

Stored at: `<object_id><0x00><0x00>`

**Versioning:**
- Incremented atomically per batch mutation
- Used for optimistic concurrency control (CAS)
- Survives individual entry deletions

#### Entry Keys
All entry keys are canonicalized to strings:
- **Numeric keys**: Converted via `u32::to_string()` (e.g., `42` → `"42"`)
- **String keys**: Stored as-is after normalization

### Storage Operations

#### Single Entry
```rust
// Set increments version automatically
await shard.set_entry("user-123", "email", bytes);

// Get returns Option<ObjectVal>
let val = await shard.get_entry("user-123", "email")?;
```

#### Batch Operations
```rust
// Atomic batch with CAS
let mut batch = HashMap::new();
batch.insert("name".into(), name_bytes);
batch.insert("age".into(), age_bytes);

let new_version = shard.batch_set_entries(
    "user-123",
    batch,
    Some(expected_version),  // CAS check
).await?;
```

**Atomicity**: All entries in a batch share the same version increment and are committed via single replication request.

#### Pagination
```rust
pub struct EntryListOptions {
    pub key_prefix: Option<String>,
    pub limit: usize,
    pub cursor: Option<Vec<u8>>,
}

pub struct EntryListResult {
    pub entries: Vec<(String, ObjectVal)>,
    pub next_cursor: Option<Vec<u8>>,
}
```

Enables efficient scanning of large objects without full reconstruction.

### Object Reconstruction

For compatibility with legacy APIs (gRPC GetObject), objects are reconstructed on-demand:

```rust
async fn reconstruct_object_from_entries(
    &self,
    id: &str,
    prefetch_limit: usize,
) -> Result<Option<ObjectData>>
```

**Process:**
1. Fetch metadata
2. Paginate through all entries
3. Separate numeric vs. string keys
4. Build `ObjectData` with both maps
5. Return None if tombstone or no metadata

**Performance**: O(entries/page_size) storage reads; configurable prefetch limit prevents unbounded scans.

### Benefits

- **Partial updates**: Modify single fields without full object read/write
- **Storage efficiency**: No redundant data for unchanged fields
- **Concurrent access**: Fine-grained locking possible at entry level
- **Large object support**: Paginated listing avoids memory bloat
- **Event precision**: Per-entry change detection (see Event Pipeline)

---

## 3. V2 Event Pipeline

### Overview
V2 is the current and only event pipeline, providing per-entry change notifications with action classification (Create/Update/Delete).

### Architecture

#### Mutation Context
```rust
pub struct MutationContext {
    pub object_id: String,
    pub cls_id: String,
    pub partition_id: u16,
    pub version_before: u64,
    pub version_after: u64,
    pub changed: Vec<ChangedKey>,
    pub event_config: Option<Arc<ObjectEvent>>,
}

pub struct ChangedKey {
    pub key_canonical: String,
    pub action: MutAction,  // Create | Update | Delete
}

pub enum MutAction {
    Create,   // Key did not exist before
    Update,   // Key existed and was modified
    Delete,   // Key existed and was removed
}
```

#### Dispatcher
```rust
pub struct V2Dispatcher {
    seq: AtomicU64,              // monotonic event sequence
    tx: Sender<V2QueuedEvent>,   // bounded async channel
    bcast: broadcast::Sender,    // in-process broadcast
    // metrics...
}
```

**Queue Characteristics:**
- Bounded channel (default: 1024)
- Non-blocking enqueue (drops on full with metric increment)
- Single consumer task per shard
- Broadcast channel for testing/monitoring

### Event Flow

#### 1. Capture Phase
During `set_entry` or `batch_set_entries`:

```rust
// Classify action based on old value presence
let action = if delete_flag {
    if old_exists { MutAction::Delete } else { /* skip */ }
} else {
    if old_exists { MutAction::Update } else { MutAction::Create }
};

let changed_key = ChangedKey {
    key_canonical: key.to_string(),
    action,
};
```

**Idempotency**: Deletes of non-existent keys produce no event.

#### 2. Enqueue Phase
After successful replication commit:

```rust
let ctx = MutationContext::new(
    object_id,
    class_id,
    partition_id,
    version_before,
    version_after,
    changed_keys,
).with_event_config(event_cfg);

v2_dispatcher.try_send(ctx);  // non-blocking
```

#### 3. Emission Phase
Consumer task processes queued events:

```rust
async fn run_v2_consumer(
    rx: Receiver<V2QueuedEvent>,
    dispatcher: Arc<V2Dispatcher>,
    bcast: broadcast::Sender,
) {
    while let Some(evt) = rx.recv().await {
        // 1. Broadcast for monitoring
        let _ = bcast.send(evt.clone());
        
        // 2. Evaluate triggers if processor available
        if let Some(processor) = &dispatcher.trigger_processor {
            processor.process_mutation(&evt.ctx).await;
        }
        
        // 3. Update metrics
        for key in &evt.ctx.changed {
            match key.action {
                MutAction::Create => dispatcher.emitted_create.fetch_add(1, Relaxed),
                MutAction::Update => dispatcher.emitted_update.fetch_add(1, Relaxed),
                MutAction::Delete => dispatcher.emitted_delete.fetch_add(1, Relaxed),
            };
        }
    }
}
```

### Trigger Processing

Triggers are evaluated per changed key against object event configuration:

```rust
pub struct ObjectEvent {
    pub data_trigger: HashMap<u32, DataTrigger>,      // numeric keys
    pub data_trigger_str: HashMap<String, DataTrigger>, // string keys
    // ...
}

pub struct DataTrigger {
    pub on_create: Vec<TriggerTarget>,
    pub on_update: Vec<TriggerTarget>,
    pub on_delete: Vec<TriggerTarget>,
}
```

**Matching Logic:**
1. Parse key to numeric or keep as string
2. Lookup trigger in appropriate map
3. Match action to trigger list (on_create/on_update/on_delete)
4. Invoke target functions via Zenoh

### Configuration

Environment variables:

| Variable | Purpose | Default |
|----------|---------|---------|
| `ODGM_EVENT_PIPELINE_V2` | Enable/disable V2 events | true |
| `ODGM_EVENT_QUEUE_BOUND` | Queue capacity | 1024 |
| `ODGM_EVENT_BCAST_BOUND` | Broadcast subscribers | 256 |
| `ODGM_MAX_BATCH_TRIGGER_FANOUT` | Max triggers per batch | 256 |

### Metrics

Exposed via atomic counters:

```rust
impl V2Dispatcher {
    pub fn metrics_emitted_events(&self) -> u64;
    pub fn metrics_emitted_create(&self) -> u64;
    pub fn metrics_emitted_update(&self) -> u64;
    pub fn metrics_emitted_delete(&self) -> u64;
    pub fn metrics_truncated_batches(&self) -> u64;
    pub fn metrics_queue_drops_total(&self) -> u64;
    pub fn metrics_queue_len(&self) -> u64;
}
```

OpenTelemetry integration available when enabled.

### Event Ordering

**Guarantees:**
- Monotonic sequence numbers per shard
- Events for object version N+1 emitted after version N
- Single consumer task ensures serialized processing

**Non-guarantees:**
- Cross-object ordering (each shard independent)
- Delivery guarantees (at-most-once; queue drops possible)

### Testing Support

```rust
// Subscribe to event stream
let mut rx = v2_dispatcher.subscribe();

// Await specific event
let evt = tokio::time::timeout(
    Duration::from_secs(5),
    async {
        while let Ok(evt) = rx.recv().await {
            if evt.ctx.object_id == "test-obj" {
                return evt;
            }
        }
    }
).await?;

// Assert actions
assert_eq!(evt.ctx.changed[0].action, MutAction::Create);
```

---

## Integration Example

Complete flow showing all three features:

```rust
// 1. String ID normalization
let identity = build_identity(
    None,
    Some("User-123"),
    160,
)?; // ObjectIdentity::Str("user-123")

// 2. Granular storage write
let mut batch = HashMap::new();
batch.insert("email".into(), email_bytes);
batch.insert("name".into(), name_bytes);

let new_version = shard.batch_set_entries(
    "user-123",
    batch,
    None,  // no CAS check
).await?;

// 3. Event emission
// Automatically enqueued with:
// - MutationContext { object_id: "user-123", version_after: 1, ... }
// - ChangedKey { key: "email", action: Create }
// - ChangedKey { key: "name", action: Create }

// 4. Trigger evaluation (if configured)
// Matches DataTrigger for "email" key
// Invokes registered functions via Zenoh
```

---

## Performance Benchmarks

Benchmark results from `cargo bench --bench event_bench` (optimized build):

### Event System Operations

| Operation | Time (ns) | Notes |
|-----------|-----------|-------|
| Trigger collection | 45.6 | Lookup matching triggers from ObjectEvent |
| Event context creation (numeric) | 25.7 | Create event with numeric key |
| Event context creation (string) | 34.3 | Create event with string key (33% slower) |
| Payload serialization (JSON) | 334 | Standard JSON encoding |
| Payload serialization (Protobuf) | 76 | Binary protobuf encoding (4.4x faster) |

**Key Insights:**
- String keys add ~8.6 ns overhead (negligible in practice)
- Protobuf is significantly faster for production workloads
- All operations are sub-microsecond
- Combined overhead: 150-450 ns per mutation (including serialization)
- No bottleneck for high-throughput scenarios

### Benchmark Quality
- Low outlier rate (3-5% across all tests)
- Tight confidence intervals
- Reproducible results on release builds

Run benchmarks: `cargo bench --bench bench_event`

---

## Performance Characteristics

### String IDs
- **Normalization**: O(n) where n = string length
- **Storage overhead**: Variable (1-160 bytes vs. 8 bytes for numeric)
- **Comparison**: String comparison vs. integer comparison

### Granular Storage
- **Write amplification**: Reduced (only changed entries written)
- **Read latency**: Single entry: O(1), Full object: O(entries/page_size)
- **Storage compaction**: Natural (no redundant data)
- **Concurrency**: Higher (entry-level isolation possible)

### V2 Events (Measured via Benchmarks)
- **Event context creation**: 25.7 ns (numeric keys) / 34.3 ns (string keys)
- **Trigger collection**: 45.6 ns (lookup matching triggers)
- **Payload serialization**: 76 ns (Protobuf) / 334 ns (JSON)
- **Total write path overhead**: ~150-450 ns depending on serialization format
- **Throughput**: Capable of millions of events/second on single thread
- **Backpressure**: Queue drops with metric increment (no blocking)

**Recommendation**: Use Protobuf serialization (4.4x faster than JSON)

---

## Migration Notes

### From Numeric to String IDs
No migration required—both types coexist. Applications can:
1. Start using string IDs for new objects
2. Keep numeric IDs for existing objects
3. Gradually migrate if needed (application-level logic)

### From Blob to Granular Storage
String IDs use granular storage exclusively. Numeric IDs still support legacy blob format but can use granular operations via wrapper.

### Event Pipeline
V2 is the only pipeline. Legacy Bridge (J0) object-level events have been removed.

---

## Design Rationale

### String IDs
- **User experience**: Human-readable identifiers
- **Debugging**: Easier log correlation
- **Integration**: External system key mapping

### Granular Storage
- **Cloud-native**: S3/blob storage friendly (smaller objects)
- **Partial updates**: Avoid full-object read-modify-write
- **Event precision**: Per-field change tracking

### V2 Events
- **Simplicity**: Single pipeline, clear semantics
- **Performance**: Non-blocking write path
- **Precision**: Entry-level granularity for triggers

---

## Future Enhancements

### String IDs
- Unicode normalization (NFD/NFC)
- Extended character sets
- Collision detection (namespace isolation)

### Granular Storage
- Entry-level TTL
- Conditional updates (predicates)
- Bulk operations (range delete)

### V2 Events
- Persistent event log (at-least-once delivery)
- Event filtering (reduce fanout)
- Cross-object aggregation
- CRDT-aware semantic events

---

**Document Status**: Reflects implementation as of November 2025 on `features/string-ids` branch.
