pub mod any_storage;
pub mod atomic_stats;
pub mod backends;
pub mod config;
pub mod error;
pub mod snapshot;
pub mod storage_value;
pub mod traits;
pub mod types;

#[cfg(test)]
pub mod tests;

pub use any_storage::AnyStorage;
pub use config::*;
pub use error::*;
pub use snapshot::*;
pub use storage_value::*;
pub use traits::*;
pub use types::*;

// Re-export backends for convenience
#[cfg(feature = "fjall")]
pub use backends::fjall::FjallStorage;
pub use backends::memory::MemoryStorage;
#[cfg(feature = "skiplist")]
pub use backends::skiplist::SkipListStorage;
