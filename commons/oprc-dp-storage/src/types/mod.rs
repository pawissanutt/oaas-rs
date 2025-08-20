//! Storage types module
//!
//! This module contains all the type definitions for the storage layer, organized by domain:
//! - `storage_types`: General storage backend types (stats, config, operations)
//! - `raft_types`: Raft consensus-specific types (log entries, state, membership)  
//! - `index_types`: Secondary indexing types (queries, extractors)

mod index_types;
mod raft_types;
mod storage_types;

pub use index_types::*;
pub use raft_types::*;
pub use storage_types::*;
