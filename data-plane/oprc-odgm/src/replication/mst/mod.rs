//! MST (Merkle Search Tree) based replication layer
//!
//! This module provides a replication layer that uses Merkle Search Trees
//! for efficient synchronization between nodes with Last Writer Wins (LWW)
//! conflict resolution.

mod config;
mod error;
mod mst_layer;
mod mst_network;
mod mst_traits;
mod types;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod mst_network_test;

// Re-export public types and implementations
pub use error::MstError;
pub use mst_layer::MstReplicationLayer;
pub use mst_network::{MstPageRequestHandlerImpl, ZenohMstNetworking};
pub use mst_traits::MstNetworking;
pub use types::{
    GenericLoadPageReq, GenericNetworkPage, GenericPagesResp, MstConfig, MstKey,
};

// Re-export key types for convenience
pub use merkle_search_tree::MerkleSearchTree;
