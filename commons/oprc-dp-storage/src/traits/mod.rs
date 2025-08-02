//! Storage traits module
//! 
//! This module contains all the trait definitions for the storage layer, organized by domain:
//! - `storage_backend`: Core storage backend traits (generic)
//! - `application_storage`: High-level application storage traits 
//! - `raft_storage`: Raft consensus-specific storage traits

pub mod application_storage;
pub mod raft_storage;
pub mod storage_backend;

// Re-export all public traits for convenience
pub use application_storage::*;
pub use raft_storage::*;
pub use storage_backend::*;
