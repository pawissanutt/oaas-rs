//! Redb storage backend implementation
//!
//! Feature-gated behind `redb`.

#![cfg(feature = "redb")]
mod backend;
mod transaction;

#[cfg(test)]
mod tests;

pub use backend::RedbStorage;
pub use transaction::RedbTransaction;

// Re-export for convenience
pub use redb;
