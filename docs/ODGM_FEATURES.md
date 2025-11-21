# ODGM Technical Overview

**Version:** 1.0  
**Date:** November 2025  
**Status:** Current Implementation

This document provides a technical overview of the major architectural features in the ODGM (Object Data Grid Manager) system.

---

## 1. String-Based Object Identifiers

### Overview
ODGM supports both numeric (u64) and string-based object identifiers. String IDs enable human-readable object references and align with modern distributed system patterns.

### Design
*   **Identity Types**: `Numeric(u64)` or `Str(String)` (normalized).
*   **Normalization**: ASCII lowercase, `[a-z0-9._:-]+`, max 160 chars.
*   **Storage**: Composite key `<object_id_utf8><0x00><record_type>`.
*   **Network**: Zenoh topic `oprc/<class>/<partition>/objects/<object_id>`.

### Compatibility
*   **Numeric IDs**: Legacy 8-byte big-endian keys remain unchanged.
*   **String IDs**: New composite key format.
*   **Coexistence**: Both types coexist; no data migration required.

---

## 2. Granular Per-Entry Storage

### Overview
Granular storage replaces object-blob serialization with fine-grained per-entry persistence. Each object field is stored independently with atomic update semantics.

### Architecture
*   **Core Trait**: `EntryStore` defines async CRUD, batch operations, and pagination.
*   **Metadata**: Stored separately (`<object_id><0x00><0x00>`) with versioning for optimistic concurrency (CAS).
*   **Entry Keys**: All keys canonicalized to strings.

### Benefits
*   **Partial Updates**: Modify single fields without full object read/write.
*   **Efficiency**: No redundant data for unchanged fields.
*   **Concurrency**: Fine-grained locking possible.
*   **Large Objects**: Paginated listing avoids memory bloat.

---

## 3. Reactive Event System

### Overview
The Reactive Event System (formerly V2 Pipeline) provides per-entry change notifications with action classification (Create/Update/Delete).

### Architecture
*   **Mutation Context**: Captures `object_id`, `version`, and `changed_keys` (with actions).
*   **Dispatcher**: Bounded async channel with non-blocking enqueue. Drops events on overflow (monitored via metrics).
*   **Triggers**: Evaluated per changed key. Supports `on_create`, `on_update`, `on_delete` hooks invoking functions via Zenoh.

### Key Features
*   **Idempotency**: Deletes of non-existent keys produce no event.
*   **Ordering**: Monotonic sequence numbers per shard.
*   **Performance**: Sub-microsecond overhead.

---

## 4. Pluggable Storage Engine

### Overview
ODGM utilizes a modular storage abstraction (`oprc-dp-storage`) allowing the underlying storage engine to be swapped based on workload requirements.

### Supported Backends
| Backend | Type | Use Case |
|---------|------|----------|
| **Memory** | In-Memory (HashMap) | Testing, Ephemeral Cache |
| **Skiplist** | In-Memory (Ordered) | High-concurrency ordered access |
| **Redb** | Embedded B+Tree | Read-heavy persistent workloads |
| **Fjall** | Embedded LSM Tree | Write-heavy persistent workloads |
| **RocksDB** | External Lib | Legacy compatibility / High performance |

### Optimization
`StorageValue` minimizes allocations by storing small values (≤ 64B) inline using `SmallVec` and large values as `bytes::Bytes`.

---

## 5. Replication & Consistency

### Overview
ODGM supports multiple replication models to balance consistency, availability, and partition tolerance.

### Models
1.  **No Replication**: Single node. For dev/cache.
2.  **Raft Consensus**: Leader-based, strong consistency (Linearizability). For critical state.
3.  **Merkle Search Tree (MST) + LWW**: Anti-entropy sync, eventual consistency. For high availability active-active scenarios.

### Sharding
Data is partitioned into shards based on `ShardId` (derived from object ID hash), managed by a `UnifiedShardManager`.

---

## 6. Performance

### Benchmarks (Event System)
*   **Event Creation**: ~25-35 ns.
*   **Trigger Collection**: ~45 ns.
*   **Serialization**: ~76 ns (Protobuf) vs ~334 ns (JSON).
*   **Total Overhead**: ~150-450 ns per mutation.

### Characteristics
*   **String IDs**: O(n) normalization overhead (negligible).
*   **Granular Storage**: Reduced write amplification; O(1) single entry read.
*   **Throughput**: Capable of millions of events/second on single thread.

---

## 7. Future Enhancements

*   **String IDs**: Unicode normalization, collision detection.
*   **Granular Storage**: Entry-level TTL, conditional updates.
*   **Event System**: Persistent event log, event filtering, CRDT-aware events.
