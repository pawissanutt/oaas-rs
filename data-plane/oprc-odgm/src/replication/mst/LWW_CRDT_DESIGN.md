# MST Replication Layer Design Document

## Executive Summary

This document outlines the design for an MST (Merkle Search Tree) based replication layer that uses **Last Writer Wins (LWW)** for conflict resolution with **anti-entropy synchronization**. The approach prioritizes simplicity, efficient synchronization, and data integrity through cryptographic hashing.

## Core Design Principles

### 1. **Efficient Synchronization**
- Use Merkle tree root hash comparison for sync detection
- Only synchronize differences, not entire datasets
- Gossip protocol for peer communication
- Anti-entropy mechanism for periodic consistency checks

### 2. **Simple Conflict Resolution**
- Last Writer Wins (LWW) based on timestamps
- Optional custom merge functions for specific data types
- Deterministic tiebreaking using node IDs
- Eventual consistency model

### 3. **Data Integrity**
- Cryptographic hashes ensure data integrity
- Merkle proofs for verification
- Tamper detection through hash mismatches
- Efficient difference identification

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                   Client Applications                       │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                MST Replication Cluster                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   Node A    │  │   Node B    │  │   Node C    │        │
│  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │        │
│  │ │Root Hash│ │  │ │Root Hash│ │  │ │Root Hash│ │        │
│  │ │  ABC123 │◄┼──┼►│  ABC123 │◄┼──┼►│  DEF456 │ │        │
│  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │        │
│  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │        │
│  │ │Merkle   │ │  │ │Merkle   │ │  │ │Merkle   │ │        │
│  │ │Tree     │ │  │ │Tree     │ │  │ │Tree     │ │        │
│  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │        │
│  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │        │
│  │ │Key-Value│ │  │ │Key-Value│ │  │ │Key-Value│ │        │
│  │ │Storage  │ │  │ │Storage  │ │  │ │Storage  │ │        │
│  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
└─────────────────────────────────────────────────────────────┘
        ▲                    ▲                    ▲
        │                    │                    │
    Gossip Protocol     Hash Comparison     Anti-Entropy Sync
```

## Key Components

### 1. **Data Storage and Indexing**
- **Key-Value Store**: Data stored as key-value pairs in local storage
- **Merkle Search Tree**: Keys organized in sorted Merkle tree structure
- **Leaf Nodes**: Each leaf represents a key-value pair with hash of that pair
- **Internal Nodes**: Store combined hashes of children up to single root hash
- **Cryptographic Integrity**: Hash-based tamper detection and verification

### 2. **Conflict Resolution**
```rust
// Simple Last Writer Wins approach
enum MergeStrategy {
    LastWriterWins,                     // Default: use timestamp + node_id
    Custom(CustomMergeFunction),        // Optional custom merge for specific types
}

type CustomMergeFunction = fn(&ObjectEntry, &ObjectEntry) -> ObjectEntry;

// Conflict resolution logic
fn resolve_conflict(local: &ObjectEntry, remote: &ObjectEntry) -> ObjectEntry {
    match strategy {
        MergeStrategy::LastWriterWins => {
            if remote.timestamp > local.timestamp {
                remote.clone()
            } else if remote.timestamp == local.timestamp {
                // Deterministic tiebreaking using node ID
                if remote.node_id > local.node_id { remote.clone() } else { local.clone() }
            } else {
                local.clone()
            }
        }
        MergeStrategy::Custom(merge_fn) => merge_fn(local, remote),
    }
}
```

### 3. **Synchronization Protocol**
- **Gossip Communication**: Nodes exchange root hashes via gossip protocol
- **Hash Comparison**: Efficient sync detection through root hash comparison
- **Delta Synchronization**: Only exchange different parts of Merkle tree
- **Anti-Entropy**: Periodic consistency checks and repair
- **Merkle Proofs**: Optional verification for high consistency guarantees

## Operational Flow

### 1. **Write Operations** ✍️
```
1. Client sends write request to any node (coordinator)
2. Coordinator performs write operation locally:
   - Update key-value store
   - Timestamp the operation
   - Recalculate affected Merkle tree hashes up to root
3. Gossip new root hash to peer nodes
4. Peers compare received hash with their own root hash
5. If hashes differ, peers request synchronization
6. Coordinator sends tree differences to out-of-sync peers
7. Peers apply changes and update their Merkle trees
```

### 2. **Read Operations** 📖
```
1. Client sends read request for specific key to any node
2. Node retrieves value from local key-value store
3. Return value to client
4. Optional: Provide Merkle proof for verification
   - Hash path from leaf to root
   - Client can verify data integrity
```

### 3. **Synchronization Process**
```
1. Periodic anti-entropy: Nodes exchange root hashes
2. If root hashes differ between peers:
   a. Identify divergent subtrees through binary search
   b. Request missing/different leaf nodes
   c. Apply changes with conflict resolution (LWW)
   d. Update Merkle tree hashes
   e. Confirm synchronization complete
3. Gossip updated root hash to other peers
```

## Data Types and Merge Semantics

### 1. **ObjectEntry Structure**
```rust
ObjectEntry {
    timestamp: u64,                     // Write timestamp for LWW
    node_id: u64,                      // Node ID for deterministic tiebreaking
    value: BTreeMap<u32, ObjectVal>,   // Object fields
    event: Option<ObjectEvent>,        // Associated events
}

ObjectVal {
    data: Vec<u8>,                     // Serialized value
    val_type: ValType,                 // Type hint for custom merge strategies
}

// Merkle tree node structure
MerkleNode {
    key_range: (Vec<u8>, Vec<u8>),     // Key range covered by this node
    hash: [u8; 32],                   // SHA-256 hash of node contents
    children: Option<Vec<MerkleNode>>, // Child nodes (None for leaf)
    data: Option<ObjectEntry>,         // Actual data (leaf nodes only)
}
```

### 2. **Merge Strategy Examples**

**Last Writer Wins (Default)**:
```rust
fn lww_merge(local: &ObjectEntry, remote: &ObjectEntry) -> ObjectEntry {
    match remote.timestamp.cmp(&local.timestamp) {
        Ordering::Greater => remote.clone(),
        Ordering::Less => local.clone(),
        Ordering::Equal => {
            // Deterministic tiebreaking
            if remote.node_id > local.node_id {
                remote.clone()
            } else {
                local.clone()
            }
        }
    }
}
```

**Optional Custom Merge Functions**:
```rust
// For additive counters
fn additive_merge(local: &ObjectEntry, remote: &ObjectEntry) -> ObjectEntry {
    // Deserialize, add values, use latest timestamp
    // Implementation details omitted for brevity
}

// For set union
fn set_union_merge(local: &ObjectEntry, remote: &ObjectEntry) -> ObjectEntry {
    // Union arrays/sets, use latest timestamp
    // Implementation details omitted for brevity
}
```

## Configuration Model

### 1. **Shard-Level Configuration**
```yaml
replication:
  type: mst
  default_strategy: "lww"              # or "custom"
  sync_interval: 30s
  type_overrides:
    counter: "additive"
    set: "set_union"
    document: "automerge_crdt"
```

### 2. **Runtime Configuration**
```rust
MergeConfig {
    default_strategy: MergeStrategy::LastWriterWins,
    type_overrides: HashMap<ValType, MergeStrategy>,
    encoding: SerializationFormat::Bincode,
    enable_compression: true,
}

// Example configuration
let config = MergeConfig {
    default_strategy: MergeStrategy::LastWriterWins,
    type_overrides: {
        ValType::Counter => MergeStrategy::Custom(additive_merge),
        ValType::Set => MergeStrategy::Custom(set_union_merge),
        ValType::Document => MergeStrategy::Custom(automerge_crdt_merge),
    },
    ..Default::default()
};
```

## Performance Characteristics

### 1. **Advantages of This Design**
✅ **Efficient Synchronization**: Merkle trees enable very efficient sync between nodes
- Only need to compare root hashes instead of entire datasets
- Binary search to identify divergent subtrees
- Only transfer actual differences

✅ **Data Integrity**: Cryptographic hashes ensure data integrity
- Any tampering immediately detectable through hash mismatches
- Merkle proofs provide verifiable data authenticity
- Hash-based tamper detection

✅ **Scalability**: Gossip-based communication is highly scalable
- Decentralized communication pattern
- No single point of failure
- Efficient bandwidth usage through delta synchronization

✅ **Simplicity**: LWW conflict resolution is easy to understand and implement
- No complex vector clock management
- Deterministic conflict resolution
- Predictable behavior

### 2. **Time Complexity**
- **Write Operation**: O(log n) for Merkle tree update
- **Read Operation**: O(log n) for key lookup
- **Sync Detection**: O(1) root hash comparison
- **Synchronization**: O(log n + d) where d = differences
- **Conflict Resolution**: O(1) for LWW, O(data_size) for custom merge

### 3. **Space Complexity**
- **Merkle Tree**: O(n) where n = number of keys
- **Hash Storage**: O(tree_height) ≈ O(log n) per path
- **Sync State**: O(peers) for gossip protocol
- **Memory Overhead**: Minimal additional storage for hashes

### 4. **Network Efficiency**
- **Hash Gossip**: Very small messages (32 bytes per root hash)
- **Delta Sync**: Only differences transmitted
- **Compression**: Tree structure naturally compresses well
- **Bandwidth**: O(differences) not O(total_data)

## Considerations and Trade-offs

### 1. **Write Overhead**
- **Hash Recalculation**: Every write requires updating hashes up to root
- **Tree Rebalancing**: Maintaining sorted tree structure adds complexity
- **Mitigation**: Batch writes and use efficient hash algorithms

### 2. **Complexity**
- **Merkle Tree Implementation**: Non-trivial data structure to implement correctly
- **Synchronization Logic**: Efficient diff calculation requires careful design
- **Mitigation**: Use proven libraries and algorithms, comprehensive testing

### 3. **Consistency Model**
- **Eventual Consistency**: No strong consistency guarantees
- **Conflict Resolution**: LWW may lose concurrent updates
- **Mitigation**: Use custom merge functions for critical data types

### 4. **Network Partitions**
- **Split-Brain**: Partitioned nodes can diverge significantly
- **Reconciliation**: Large divergences may require expensive synchronization
- **Mitigation**: Anti-entropy and conflict resolution handle reunification

## Implementation Strategy

### Phase 1: Core MST Infrastructure
1. Implement basic Merkle Search Tree with hash calculation
2. Add simple LWW conflict resolution
3. Implement basic key-value operations (get/put/delete)
4. Add root hash calculation and comparison

### Phase 2: Synchronization Protocol
1. Implement gossip protocol for hash exchange
2. Add delta synchronization logic
3. Implement tree diff calculation
4. Add anti-entropy background process

### Phase 3: Advanced Features
1. Add Merkle proof generation and verification
2. Implement custom merge functions
3. Add compression and optimization
4. Performance tuning and monitoring

### Phase 4: Production Features
1. Add administrative tools and monitoring
2. Implement backup and recovery
3. Add metrics and observability
4. Performance optimization and tuning

## Benefits of This Approach

### 1. **Practical Simplicity**
✅ No complex vector clock management  
✅ Builds on existing `merge_cloned()` pattern  
✅ Familiar timestamp-based ordering  
✅ Easy to understand and debug  

### 2. **Performance**
✅ Minimal overhead for common operations  
✅ Efficient binary serialization  
✅ Lazy conflict resolution  
✅ Optimized for the common case (no conflicts)  

### 3. **Flexibility**
✅ Multiple merge strategies per data type  
✅ Configurable per shard or key pattern  
✅ Pluggable custom merge functions  
✅ Gradual adoption (start with LWW, add CRDTs)  

### 4. **Integration**
✅ Leverages existing MST implementation  
✅ Compatible with current storage layer  
✅ Works with existing Zenoh networking  
✅ Minimal changes to application code  

## Risk Mitigation

### 1. **Clock Skew**
- Use logical timestamps instead of wall time
- Implement hybrid logical clocks (HLC) if needed
- Fallback to deterministic tiebreaking (node ID)

### 2. **Network Partitions**
- MST enables offline operation and later sync
- Conflict resolution handles partition merging
- Eventual consistency guarantees

### 3. **Data Corruption**
- CRDT merge functions are associative/commutative
- MST provides integrity checking via hashes
- Rollback capability for failed merges

### 4. **Performance Degradation**
- Configurable strategies allow performance tuning
- Lazy evaluation reduces overhead
- Compression reduces network impact

## Success Metrics

1. **Correctness**: All nodes converge to same state
2. **Performance**: < 1ms conflict resolution for simple cases
3. **Scalability**: Support 100+ nodes per shard
4. **Reliability**: 99.9% successful conflict resolution
5. **Usability**: Zero-configuration for common cases

This design provides a solid foundation for implementing efficient, practical replication with sophisticated conflict resolution while maintaining simplicity and performance.
