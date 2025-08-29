pub mod error;
pub mod traits;

#[cfg(feature = "etcd")]
pub mod etcd;

#[cfg(feature = "memory")]
pub mod memory;

// Always compile the unified facade; it internally gates per-feature.
pub mod unified;

pub use error::*;
pub use traits::*;
