# OaaS-RS Copilot Instructions

## Project Overview

This is a Rust reimplementation of the Oparaca Object-as-a-Service platform, focusing on the data plane while maintaining Java control plane compatibility. The system replaces the original Java "Invoker" with ODGM (Object Data Grid Manager) in Rust.

## Architecture & Key Components

### Core Data Plane Components
- **ODGM** (`data-plane/oprc-odgm/`): Main object data grid manager with distributed sharding
- **Gateway** (`data-plane/oprc-gateway/`): HTTP/gRPC gateway for external requests  
- **Router** (`data-plane/oprc-router/`): Zenoh-based message routing
- **Storage Layer** (`commons/oprc-dp-storage/`): Pluggable storage backends (memory, persistent)

### Communication Stack
- **Zenoh**: Primary pub/sub messaging system (not MQTT/Redis)
- **ZRPC**: Custom RPC layer built on Zenoh via `flare-zrpc` library
- **gRPC**: External API interfaces using Protocol Buffers

### Key Networking Patterns
```rust
// Zenoh session creation (standard pattern)
let z_config = oprc_zenoh::OprcZenohConfig::init_from_env()?;
let session = zenoh::open(z_config.create_zenoh()).await?;

// ZRPC service pattern
let client = ZrpcClient::new(service_id, session.clone()).await;
let result = client.call_with_key(key, request).await?;
```

## Essential Development Patterns

### Shard Management
Shards are distributed storage units with multiple replication strategies:
- **Raft Consensus**: `data-plane/oprc-odgm/src/replication/raft/` for strong consistency
- **MST (Merkle Search Tree)**: `data-plane/oprc-odgm/src/shard/mst/` for eventual consistency
- **Basic Shards**: Memory-only storage without replication

### Storage Backend Pattern
```rust
// All storage implements ApplicationDataStorage trait
impl ApplicationDataStorage for MyStorage {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>>;
    async fn put(&self, key: &[u8], value: StorageValue) -> StorageResult<()>;
}

// Enhanced storage supports snapshots for Raft
impl SnapshotCapableStorage for MyStorage {
    async fn create_zero_copy_snapshot(&self) -> StorageResult<ZeroCopySnapshot>;
}
```

### Replication Layer Integration
Use `ObjectUnifiedShard` with pluggable replication:
```rust
let shard = ObjectUnifiedShard::new_full(
    metadata,
    app_storage,        // ApplicationDataStorage
    Some(replication),  // ReplicationLayer (Raft, MST, etc.)
    z_session,
    event_manager,
).await?;
```

## Build & Development

### Essential Commands
```bash
# Build release version
cargo build -r

# Container build (preferred for complete system)
docker compose -f docker-compose.release.yml build

# Development with Just
just dev-up           # Build and start dev containers
just compose-dev       # Start development environment
```

### Configuration
- Environment-based config via `envconfig` crate
- Zenoh peer discovery through `OPRC_ZENOH_PEERS` env var
- Storage backends configurable per shard via metadata options

## Critical Implementation Notes

### Raft Consensus Details
- Uses OpenRaft library with custom `ObjectShardStateMachine`
- State machine integrates with `SnapshotCapableStorage` for zero-copy snapshots
- RPC operations auto-forward to leader via `RaftOperationManager`
- Leader election timeouts: 200-2000ms, heartbeat: 100ms

### Storage Architecture
- `MemoryStorage` for development (in `commons/oprc-dp-storage/src/backends/memory.rs`)
- Transaction support through `StorageTransaction` trait
- Key-value interface with byte arrays, not typed keys

### Networking Specifics
- Zenoh modes: Client, Peer, Router (configured via `OPRC_ZENOH_MODE`)
- Service discovery through Zenoh liveliness tokens
- QoS settings: `CongestionControl::Block` for reliability
- Managed concurrency via `declare_managed_queryable` pattern

### Testing Patterns
- Use `#[tokio::test(flavor = "multi_thread")]` for async tests
- Mock Zenoh sessions for unit testing
- Real etcd instances for integration tests

## Key Files for Reference
- `data-plane/oprc-odgm/src/shard/unified/object_shard.rs`: Main shard implementation
- `data-plane/oprc-odgm/src/replication/raft/raft_layer.rs`: Raft integration
- `commons/oprc-zenoh/src/lib.rs`: Zenoh configuration patterns
- `data-plane/oprc-odgm/src/shard/raft/rpc.rs`: Leader forwarding logic

When working with this codebase, prioritize understanding the Zenoh communication patterns and storage/replication abstractions as they form the foundation for all distributed operations.
