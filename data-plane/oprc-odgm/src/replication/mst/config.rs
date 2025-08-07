//! Configuration utilities for MST replication

use super::types::MstConfig;
use oprc_dp_storage::StorageError;

/// Helper function to create a simple LWW merge configuration for types with timestamps
impl<T> MstConfig<T>
where
    T: Clone
        + Send
        + Sync
        + 'static
        + serde::Serialize
        + for<'de> serde::Deserialize<'de>,
{
    /// Create a simple JSON-based configuration with LWW merge
    pub fn simple_lww(extract_timestamp: fn(&T) -> u64) -> Self {
        Self {
            extract_timestamp: Box::new(extract_timestamp),
            merge_function: Box::new(
                move |local: &T, remote: &T, node_id: u64| {
                    use std::cmp::Ordering;

                    let local_ts = extract_timestamp(local);
                    let remote_ts = extract_timestamp(remote);

                    match remote_ts.cmp(&local_ts) {
                        Ordering::Greater => remote.clone(),
                        Ordering::Less => local.clone(),
                        Ordering::Equal => {
                            // Deterministic tiebreaking using node_id
                            if node_id % 2 == 0 {
                                remote.clone()
                            } else {
                                local.clone()
                            }
                        }
                    }
                },
            ),
            serialize: Box::new(|value| {
                serde_json::to_vec(value)
                    .map_err(|e| StorageError::serialization(&e.to_string()))
            }),
            deserialize: Box::new(|bytes| {
                serde_json::from_slice(bytes)
                    .map_err(|e| StorageError::serialization(&e.to_string()))
            }),
        }
    }
}
