//! MST (Merkle Search Tree) based replication layer
//! 
//! This module provides a replication layer that uses Merkle Search Trees
//! for efficient synchronization between nodes with Last Writer Wins (LWW)
//! conflict resolution.

pub mod layer;

pub use layer::{MstReplicationLayer, MstKey};

// Re-export key types for convenience
pub use merkle_search_tree::MerkleSearchTree;
