//! MST (Merkle Search Tree) based replication layer
//!
//! This module provides a replication layer that uses Merkle Search Trees
//! for efficient synchronization between nodes with Last Writer Wins (LWW)
//! conflict resolution.

mod config;
mod error;
mod layer;
mod networking;
mod traits;
mod types;

#[cfg(test)]
mod tests;

// Re-export public types and implementations
pub use error::MstError;
pub use layer::MstReplicationLayer;
pub use networking::{ZenohMstNetworking, MstPageRequestHandlerImpl, MstPageUpdateHandlerImpl};
pub use traits::{MstNetworking, MstPageRequestHandler, MstPageUpdateHandler};
pub use types::{MstConfig, MstKey, GenericNetworkPage, GenericLoadPageReq, GenericPagesResp};

// Re-export key types for convenience
pub use merkle_search_tree::MerkleSearchTree;
