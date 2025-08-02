//! Storage types module
//! 
//! This module contains all the type definitions for the storage layer, organized by domain:
//! - `storage_types`: General storage backend types (stats, config, operations)
//! - `raft_types`: Raft consensus-specific types (log entries, state, membership)  
//! - `index_types`: Secondary indexing types (queries, extractors)

pub mod index_types;
pub mod raft_types;
pub mod storage_types;

// Re-export all public types for convenience
pub use index_types::*;
pub use raft_types::*;
pub use storage_types::*;
