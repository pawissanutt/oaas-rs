mod basic;
mod event;
mod raft;
mod weak;

use automerge::AutomergeError;
pub use basic::ObjectEntry;
pub use basic::ObjectShard;
pub use basic::ObjectShardFactory;
pub use basic::ObjectVal;

#[derive(thiserror::Error, Debug)]
pub enum ShardError {
    #[error("Merge Error: {0}")]
    MergeError(#[from] AutomergeError),
}
