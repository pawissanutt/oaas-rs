# OaaS-RS Storage Architecture

The OaaS-RS system uses a dual storage architecture that separates control plane operations from data plane operations, following the principle of clear separation of concerns.

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        OaaS-RS System                          │
├─────────────────────────────────────────────────────────────────┤
│  Control Plane                    │  Data Plane                │
│  ┌─────────────────────┐         │  ┌─────────────────────┐   │
│  │   oprc-cp-storage   │         │  │   oprc-dp-storage   │   │
│  │                     │         │  │                     │   │
│  │ • Package metadata  │         │  │ • Object storage    │   │
│  │ • Class definitions │         │  │ • Sharded data      │   │
│  │ • Deployment specs  │         │  │ • Raft consensus    │   │
│  │ • Runtime state     │         │  │ • MST replication   │   │
│  │                     │         │  │ • Memory/Persistent │   │
│  └─────────────────────┘         │  └─────────────────────┘   │
│           │                      │           │               │
│           ▼                      │           ▼               │
│  ┌─────────────────────┐         │  ┌─────────────────────┐   │
│  │        etcd         │         │  │  Various Backends   │   │
│  │   (Primary Store)   │         │  │  (Memory, Files,    │   │
│  │                     │         │  │   Database, etc.)   │   │
│  └─────────────────────┘         │  └─────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## Control Plane Storage (oprc-cp-storage)

### Purpose
Manages metadata and configuration for the OaaS system, including:
- **Package Definitions**: Function packages, dependencies, and metadata
- **Class Definitions**: Object class schemas and configurations
- **Deployment Specifications**: How and where services should be deployed
- **Runtime State**: Active deployments, service health, and system state

### Characteristics
- **Consistency**: Strong consistency requirements for metadata
- **Durability**: Must persist across system restarts
- **Scalability**: Moderate scale (thousands of packages/classes)
- **Access Pattern**: Read-heavy workload with occasional updates
- **Primary Backend**: etcd (distributed key-value store)

### Module Structure
```
commons/oprc-cp-storage/
├── src/
│   ├── lib.rs              // Main module exports
│   ├── traits.rs           // Storage trait definitions
│   ├── error.rs            // Error types
│   ├── memory/             // In-memory implementation
│   │   └── mod.rs
│   └── etcd/               // etcd implementation
│       └── mod.rs
└── Cargo.toml
```

### Key Traits
```rust
// Package storage operations
#[async_trait]
pub trait PackageStorage: Send + Sync {
    async fn store_package(&self, package: &OPackage) -> StorageResult<()>;
    async fn get_package(&self, id: &str) -> StorageResult<Option<OPackage>>;
    async fn list_packages(&self, filter: &PackageFilter) -> StorageResult<Vec<OPackage>>;
    async fn delete_package(&self, id: &str) -> StorageResult<()>;
}

// Deployment storage operations
#[async_trait]
pub trait DeploymentStorage: Send + Sync {
    async fn store_deployment(&self, deployment: &OClassDeployment) -> StorageResult<()>;
    async fn get_deployment(&self, id: &str) -> StorageResult<Option<OClassDeployment>>;
    async fn list_deployments(&self, filter: &DeploymentFilter) -> StorageResult<Vec<OClassDeployment>>;
    async fn delete_deployment(&self, id: &str) -> StorageResult<()>;
}

// Runtime state storage operations
#[async_trait]
pub trait RuntimeStateStorage: Send + Sync {
    async fn store_runtime_state(&self, state: &RuntimeState) -> StorageResult<()>;
    async fn get_runtime_state(&self, id: &str) -> StorageResult<Option<RuntimeState>>;
    async fn list_runtime_states(&self, filter: &RuntimeFilter) -> StorageResult<Vec<RuntimeState>>;
    async fn delete_runtime_state(&self, id: &str) -> StorageResult<()>;
}
```

## Data Plane Storage (oprc-dp-storage)

### Purpose
Handles high-performance object data storage and retrieval for the ODGM (Object Data Grid Manager):
- **Object Data**: Application objects and their state
- **Sharded Storage**: Distributed across multiple nodes
- **Replication**: Multiple consistency models (Raft, MST)
- **Performance**: Optimized for high-throughput object operations

### Characteristics
- **Performance**: Optimized for high-throughput, low-latency access
- **Scalability**: Massive scale (millions of objects)
- **Distribution**: Sharded across multiple nodes
- **Consistency Models**: Pluggable (Strong via Raft, Eventual via MST)
- **Backends**: Memory, Persistent files, Databases

### Module Structure
```
commons/oprc-dp-storage/
├── src/
│   ├── lib.rs              // Main module exports
│   ├── traits.rs           // Storage trait definitions
│   ├── backends/           // Storage backend implementations
│   │   ├── memory.rs       // In-memory storage
│   │   ├── persistent.rs   // File-based storage
│   │   └── mod.rs
│   ├── transaction.rs      // Transaction support
│   └── snapshot.rs         // Snapshot capabilities
└── Cargo.toml
```

### Key Traits
```rust
// Core object storage operations
#[async_trait]
pub trait ApplicationDataStorage: Send + Sync {
    async fn get(&self, key: &[u8]) -> StorageResult<Option<StorageValue>>;
    async fn put(&self, key: &[u8], value: StorageValue) -> StorageResult<()>;
    async fn delete(&self, key: &[u8]) -> StorageResult<()>;
    async fn scan(&self, start: &[u8], end: &[u8]) -> StorageResult<Vec<(Vec<u8>, StorageValue)>>;
}

// Enhanced storage with snapshots for replication
#[async_trait]
pub trait SnapshotCapableStorage: ApplicationDataStorage {
    async fn create_zero_copy_snapshot(&self) -> StorageResult<ZeroCopySnapshot>;
    async fn restore_from_snapshot(&self, snapshot: ZeroCopySnapshot) -> StorageResult<()>;
}

// Transaction support
#[async_trait]
pub trait StorageTransaction: Send + Sync {
    async fn commit(self: Box<Self>) -> StorageResult<()>;
    async fn rollback(self: Box<Self>) -> StorageResult<()>;
}
```

## Integration Patterns

### Control Plane Usage
```rust
use oprc_cp_storage::{StorageFactory, etcd::EtcdStorageFactory};
use oprc_models::{OPackage, OClassDeployment};

// Initialize control plane storage
let factory = EtcdStorageFactory::new("etcd://localhost:2379").await?;
let package_storage = factory.create_package_storage().await?;

// Store package metadata
let package = OPackage {
    id: "my-function".to_string(),
    // ... package details
};
package_storage.store_package(&package).await?;
```

### Data Plane Usage
```rust
use oprc_dp_storage::{MemoryStorage, ApplicationDataStorage};

// Initialize data plane storage
let storage = MemoryStorage::new();

// Store object data
let key = b"object:123";
let value = StorageValue::new(b"object data".to_vec());
storage.put(key, value).await?;
```

## Storage Selection Guidelines

### Use Control Plane Storage (oprc-cp-storage) For:
- Package and class metadata
- Deployment configurations
- System configuration
- Audit logs
- Service discovery information
- Control plane state management

### Use Data Plane Storage (oprc-dp-storage) For:
- Application object data
- High-frequency read/write operations
- Distributed caching
- Temporary data
- Performance-critical operations
- Sharded data requirements

## Migration and Consistency

### Data Flow
1. **Package Upload**: Metadata goes to CP storage, binaries may go to DP storage
2. **Object Creation**: Object metadata in CP storage, object data in DP storage
3. **Runtime Operations**: State updates in CP storage, data operations in DP storage

### Consistency Considerations
- **CP Storage**: Strong consistency via etcd's Raft consensus
- **DP Storage**: Configurable consistency (Raft for strong, MST for eventual)
- **Cross-Storage**: Application-level consistency management required

## Configuration Examples

### Control Plane Storage Configuration
```rust
[storage.control_plane]
backend = "etcd"
endpoints = ["http://etcd1:2379", "http://etcd2:2379", "http://etcd3:2379"]
timeout = "5s"
retry_attempts = 3
```

### Data Plane Storage Configuration
```rust
[storage.data_plane]
backend = "memory"  # or "persistent", "database"
replication = "raft"  # or "mst", "none"
shard_count = 16
snapshot_interval = "1h"
```

This dual storage architecture provides the OaaS-RS system with the flexibility to optimize for both metadata management and high-performance object operations while maintaining clear separation of concerns.
