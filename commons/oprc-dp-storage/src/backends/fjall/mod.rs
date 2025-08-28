//! Fjall storage backend implementation
//!
//! This module provides a persistent storage backend using the Fjall database.
//! Fjall is a LSM-tree based embedded database with ACID transactions.

mod backend;
mod snapshot;
mod transaction;
mod tx_backend;
mod tx_transaction;

#[cfg(test)]
mod tests;

pub use backend::FjallStorage;
pub use snapshot::{FjallSnapshotData, FjallSnapshotStream};
pub use transaction::FjallTransaction;
pub use tx_backend::FjallTxStorage;
pub use tx_transaction::FjallTxTransaction;

// Re-export for convenience
pub use fjall;
