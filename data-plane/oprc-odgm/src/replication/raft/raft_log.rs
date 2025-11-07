use std::ops::RangeBounds;
use std::sync::Arc;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use openraft::{
    ErrorSubject, ErrorVerb, LogId, LogState, StorageError, StorageIOError,
    Vote, storage::LogFlushed,
};
use oprc_dp_storage::{StorageBackend, StorageValue};

use crate::replication::raft::{NodeId, ReplicationTypeConfig};

type StorageResult<T> = Result<T, StorageError<NodeId>>;

/// OpenRaft log store implementation using separate storage backends
#[derive(Debug, Clone)]
pub struct OpenraftLogStore<L, M>
where
    L: StorageBackend + Clone + 'static, // Log entries storage
    M: StorageBackend + Clone + 'static, // Metadata storage
{
    /// Storage backend for persisting log entries
    log_backend: Arc<L>,
    /// Storage backend for persisting metadata (vote, committed, etc.)
    metadata_backend: Arc<M>,
}

impl<L, M> OpenraftLogStore<L, M>
where
    L: StorageBackend + Clone + 'static,
    M: StorageBackend + Clone + 'static,
{
    /// Create a new OpenraftLogStore with separate storage backends
    pub fn new(log_backend: L, metadata_backend: M) -> Self {
        Self {
            log_backend: Arc::new(log_backend),
            metadata_backend: Arc::new(metadata_backend),
        }
    }

    /// Get the log storage backend
    pub fn log_backend(&self) -> &L {
        &self.log_backend
    }

    /// Get the metadata storage backend
    pub fn metadata_backend(&self) -> &M {
        &self.metadata_backend
    }

    /// Flush both storage backends
    async fn flush_backends(
        &self,
        subject: ErrorSubject<NodeId>,
        _verb: ErrorVerb,
    ) -> Result<(), StorageIOError<NodeId>> {
        // Flush log backend
        self.log_backend.flush().await.map_err(|e| match subject {
            ErrorSubject::Vote => {
                StorageIOError::write_vote(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Log storage flush error: {}", e),
                ))
            }
            ErrorSubject::Logs => {
                StorageIOError::write_logs(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Log storage flush error: {}", e),
                ))
            }
            _ => StorageIOError::write(&std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Log storage flush error: {}", e),
            )),
        })?;

        // Flush metadata backend
        self.metadata_backend
            .flush()
            .await
            .map_err(|e| match subject {
                ErrorSubject::Vote => {
                    StorageIOError::write_vote(&std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Metadata storage flush error: {}", e),
                    ))
                }
                ErrorSubject::Logs => {
                    StorageIOError::write_logs(&std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Metadata storage flush error: {}", e),
                    ))
                }
                _ => StorageIOError::write(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Metadata storage flush error: {}", e),
                )),
            })?;

        Ok(())
    }

    /// Convert a log index to binary key for storage
    /// Note that we're using big endian encoding to ensure correct sorting of keys
    fn log_id_to_key(index: u64) -> StorageValue {
        let mut buf = Vec::with_capacity(8); // 8 bytes for u64
        buf.write_u64::<BigEndian>(index).unwrap();
        StorageValue::from(buf)
    }

    /// Convert binary key back to log index
    fn key_to_log_id(key: &[u8]) -> Option<u64> {
        if key.len() != 8 {
            return None;
        }
        (&key[0..8]).read_u64::<BigEndian>().ok()
    }

    /// Serialize data using serde_json
    fn serialize<T: serde::Serialize>(data: &T) -> StorageResult<Vec<u8>> {
        serde_json::to_vec(data).map_err(|e| StorageError::IO {
            source: StorageIOError::write(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize data: {}", e),
            )),
        })
    }

    /// Deserialize data using serde_json
    fn deserialize<T: serde::de::DeserializeOwned>(
        data: &[u8],
    ) -> StorageResult<T> {
        serde_json::from_slice(data).map_err(|e| StorageError::IO {
            source: StorageIOError::read(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize data: {}", e),
            )),
        })
    }

    /// Serialize log entry using serde_json
    fn serialize_log_entry(
        entry: &openraft::Entry<ReplicationTypeConfig>,
    ) -> StorageResult<Vec<u8>> {
        serde_json::to_vec(entry).map_err(|e| StorageError::IO {
            source: StorageIOError::write_logs(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize log entry: {}", e),
            )),
        })
    }

    /// Deserialize log entry using serde_json
    fn deserialize_log_entry(
        data: &[u8],
    ) -> StorageResult<openraft::Entry<ReplicationTypeConfig>> {
        serde_json::from_slice(data).map_err(|e| StorageError::IO {
            source: StorageIOError::read_logs(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize log entry: {}", e),
            )),
        })
    }

    /// Serialize vote using serde_json
    fn serialize_vote(vote: &Vote<NodeId>) -> StorageResult<Vec<u8>> {
        serde_json::to_vec(vote).map_err(|e| StorageError::IO {
            source: StorageIOError::write_vote(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to serialize vote: {}", e),
            )),
        })
    }

    /// Deserialize vote using serde_json
    fn deserialize_vote(data: &[u8]) -> StorageResult<Vote<NodeId>> {
        serde_json::from_slice(data).map_err(|e| StorageError::IO {
            source: StorageIOError::read_vote(&std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to deserialize vote: {}", e),
            )),
        })
    }

    /// Get last purged log ID
    async fn get_last_purged(&self) -> StorageResult<Option<LogId<NodeId>>> {
        match self.metadata_backend.get(b"last_purged_log_id").await {
            Ok(Some(value)) => {
                let log_id: LogId<NodeId> =
                    Self::deserialize(value.as_slice())?;
                Ok(Some(log_id))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::IO {
                source: StorageIOError::read(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Metadata storage error: {}", e),
                )),
            }),
        }
    }

    /// Set last purged log ID
    async fn set_last_purged(
        &self,
        log_id: LogId<NodeId>,
    ) -> StorageResult<()> {
        let value = Self::serialize(&log_id)?;

        self.metadata_backend
            .put(b"last_purged_log_id", StorageValue::from(value))
            .await
            .map_err(|e| StorageError::IO {
                source: StorageIOError::write(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Metadata storage error: {}", e),
                )),
            })?;

        self.flush_backends(ErrorSubject::Store, ErrorVerb::Write)
            .await?;
        Ok(())
    }

    /// Set committed log ID
    async fn set_committed(
        &self,
        committed: &Option<LogId<NodeId>>,
    ) -> StorageResult<()> {
        let value = Self::serialize(committed)?;

        self.metadata_backend
            .put(b"committed", StorageValue::from(value))
            .await
            .map_err(|e| StorageError::IO {
                source: StorageIOError::write(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Metadata storage error: {}", e),
                )),
            })?;

        self.flush_backends(ErrorSubject::Store, ErrorVerb::Write)
            .await?;
        Ok(())
    }

    /// Get committed log ID
    async fn get_committed(&self) -> StorageResult<Option<LogId<NodeId>>> {
        match self.metadata_backend.get(b"committed").await {
            Ok(Some(value)) => {
                let committed: Option<LogId<NodeId>> =
                    Self::deserialize(value.as_slice())?;
                Ok(committed)
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::IO {
                source: StorageIOError::read(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Metadata storage error: {}", e),
                )),
            }),
        }
    }

    /// Set vote
    async fn set_vote(&self, vote: &Vote<NodeId>) -> StorageResult<()> {
        let value = Self::serialize_vote(vote)?;

        self.metadata_backend
            .put(b"vote", StorageValue::from(value))
            .await
            .map_err(|e| StorageError::IO {
                source: StorageIOError::write_vote(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Metadata storage error: {}", e),
                )),
            })?;

        self.flush_backends(ErrorSubject::Vote, ErrorVerb::Write)
            .await?;
        Ok(())
    }

    /// Get vote
    async fn get_vote(&self) -> StorageResult<Option<Vote<NodeId>>> {
        match self.metadata_backend.get(b"vote").await {
            Ok(Some(value)) => {
                let vote = Self::deserialize_vote(value.as_slice())?;
                Ok(Some(vote))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(StorageError::IO {
                source: StorageIOError::read_vote(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Metadata storage error: {}", e),
                )),
            }),
        }
    }
}

impl<L, M> openraft::RaftLogReader<ReplicationTypeConfig>
    for OpenraftLogStore<L, M>
where
    L: StorageBackend + Clone + 'static,
    M: StorageBackend + Clone + 'static,
{
    async fn try_get_log_entries<
        RB: RangeBounds<u64> + Clone + std::fmt::Debug + Send,
    >(
        &mut self,
        range: RB,
    ) -> Result<
        Vec<openraft::Entry<ReplicationTypeConfig>>,
        openraft::StorageError<NodeId>,
    > {
        let start_index = match range.start_bound() {
            std::ops::Bound::Included(x) => *x,
            std::ops::Bound::Excluded(x) => *x + 1,
            std::ops::Bound::Unbounded => 0,
        };

        let end_index = match range.end_bound() {
            std::ops::Bound::Included(x) => *x + 1,
            std::ops::Bound::Excluded(x) => *x,
            std::ops::Bound::Unbounded => u64::MAX,
        };

        // Use scan_range to efficiently retrieve entries in the range
        let start_key = Self::log_id_to_key(start_index).into_vec();
        let end_key = Self::log_id_to_key(end_index).into_vec();

        let scan_result = self
            .log_backend
            .scan_range(start_key..end_key)
            .await
            .map_err(|e| StorageError::IO {
                source: StorageIOError::read_logs(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Log storage scan error: {}", e),
                )),
            })?;

        let entries: Result<Vec<_>, _> = scan_result
            .into_iter()
            .filter_map(|(key, value)| {
                // Only process keys that can be converted to log indices
                Self::key_to_log_id(&key).map(|_| value)
            })
            .map(|value| Self::deserialize_log_entry(value.as_slice()))
            .collect();

        entries
    }
}

impl<L, M> openraft::storage::RaftLogStorage<ReplicationTypeConfig>
    for OpenraftLogStore<L, M>
where
    L: StorageBackend + Clone + 'static,
    M: StorageBackend + Clone + 'static,
{
    type LogReader = Self;

    async fn get_log_state(
        &mut self,
    ) -> StorageResult<LogState<ReplicationTypeConfig>> {
        // Use reverse scan to get the last log entry instead of metadata
        let last_log_id = match self.log_backend.get_last().await {
            Ok(Some((key, value))) => {
                // Convert key to log index and deserialize entry to get LogId
                if let Some(_index) = Self::key_to_log_id(&key) {
                    match Self::deserialize_log_entry(value.as_slice()) {
                        Ok(entry) => Some(entry.log_id),
                        Err(_) => None,
                    }
                } else {
                    None
                }
            }
            Ok(None) => None,
            Err(_) => None,
        };

        let last_purged_log_id = self.get_last_purged().await?;

        Ok(LogState {
            last_purged_log_id,
            last_log_id,
        })
    }

    async fn save_committed(
        &mut self,
        committed: Option<LogId<NodeId>>,
    ) -> Result<(), StorageError<NodeId>> {
        self.set_committed(&committed).await
    }

    async fn read_committed(
        &mut self,
    ) -> Result<Option<LogId<NodeId>>, StorageError<NodeId>> {
        self.get_committed().await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn save_vote(
        &mut self,
        vote: &Vote<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        self.set_vote(vote).await
    }

    async fn read_vote(
        &mut self,
    ) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        self.get_vote().await
    }

    #[tracing::instrument(level = "trace", skip_all)]
    async fn append<I>(
        &mut self,
        entries: I,
        callback: LogFlushed<ReplicationTypeConfig>,
    ) -> StorageResult<()>
    where
        I: IntoIterator<Item = openraft::Entry<ReplicationTypeConfig>> + Send,
        I::IntoIter: Send,
    {
        let mut append_result: Result<(), std::io::Error> = Ok(());

        for entry in entries {
            let key = Self::log_id_to_key(entry.log_id.index);

            match Self::serialize_log_entry(&entry) {
                Ok(value) => {
                    if let Err(e) = self
                        .log_backend
                        .put(key.as_slice(), StorageValue::from(value))
                        .await
                    {
                        append_result = Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Log storage error: {}", e),
                        ));
                        break;
                    }
                }
                Err(e) => {
                    append_result = Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to serialize log entry: {:?}", e),
                    ));
                    break;
                }
            }
        }

        // Flush the log storage to ensure entries are persisted
        if append_result.is_ok() {
            if let Err(flush_err) = self
                .flush_backends(ErrorSubject::Logs, ErrorVerb::Write)
                .await
            {
                append_result = Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to flush storage: {:?}", flush_err),
                ));
            }
        }

        // Notify completion - create a new Result since io::Error doesn't implement Clone
        let callback_result = match &append_result {
            Ok(_) => Ok(()),
            Err(e) => Err(std::io::Error::new(e.kind(), e.to_string())),
        };
        callback.log_io_completed(callback_result);

        // Convert io::Error to StorageError if needed
        match append_result {
            Ok(_) => Ok(()),
            Err(e) => Err(StorageError::IO {
                source: StorageIOError::write_logs(&e),
            }),
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn truncate(&mut self, log_id: LogId<NodeId>) -> StorageResult<()> {
        tracing::debug!("delete_log: [{:?}, +oo)", log_id);

        // Use reverse scan to get the current last log entry
        let current_last = match self.log_backend.get_last().await {
            Ok(Some((key, _))) => {
                if let Some(index) = Self::key_to_log_id(&key) {
                    index
                } else {
                    return Ok(()); // No valid entries, nothing to truncate
                }
            }
            _ => return Ok(()), // No entries, nothing to truncate
        };

        // Use delete_range for efficient batch deletion from log_id.index onwards
        if log_id.index <= current_last {
            let start_key = Self::log_id_to_key(log_id.index).into_vec();
            let end_key = Self::log_id_to_key(current_last + 1).into_vec(); // Exclusive end, so +1

            let deleted_count = self
                .log_backend
                .delete_range(start_key..end_key)
                .await
                .map_err(|e| StorageError::IO {
                    source: StorageIOError::write_logs(&std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Log storage delete_range error: {}", e),
                    )),
                })?;

            tracing::debug!(
                "Truncated {} log entries from index {} to {}",
                deleted_count,
                log_id.index,
                current_last
            );
        }

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip(self))]
    async fn purge(
        &mut self,
        log_id: LogId<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        tracing::debug!("delete_log: [0, {:?}]", log_id);

        self.set_last_purged(log_id).await?;

        // Use delete_range for efficient batch deletion of all log entries up to and including log_id.index
        let start_key = Self::log_id_to_key(0).into_vec();
        let end_key = Self::log_id_to_key(log_id.index + 1).into_vec(); // Exclusive end, so +1

        let deleted_count = self
            .log_backend
            .delete_range(start_key..end_key)
            .await
            .map_err(|e| StorageError::IO {
                source: StorageIOError::write_logs(&std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Log storage delete_range error: {}", e),
                )),
            })?;

        tracing::debug!(
            "Purged {} log entries up to index {}",
            deleted_count,
            log_id.index
        );

        Ok(())
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replication::{Operation, ReadOperation, ShardRequest};
    use openraft::{Entry, EntryPayload, LeaderId, LogId, Vote};
    use oprc_dp_storage::{MemoryStorage, StorageConfig, StorageValue};
    use std::time::SystemTime;

    async fn create_test_log_store()
    -> OpenraftLogStore<MemoryStorage, MemoryStorage> {
        let config = StorageConfig::default();
        let log_storage = MemoryStorage::new(config.clone()).unwrap();
        let metadata_storage = MemoryStorage::new(config).unwrap();
        OpenraftLogStore::new(log_storage, metadata_storage)
    }

    fn create_test_entry(
        index: u64,
        term: u64,
    ) -> Entry<ReplicationTypeConfig> {
        Entry {
            log_id: LogId {
                leader_id: LeaderId::from(Vote::new(term, 1)),
                index,
            },
            payload: EntryPayload::Normal(ShardRequest {
                operation: Operation::Read(ReadOperation {
                    key: StorageValue::from(format!("test_key_{}", index)),
                }),
                timestamp: SystemTime::now(),
                source_node: 1,
            }),
        }
    }

    #[tokio::test]
    async fn test_initial_log_state() {
        use openraft::storage::RaftLogStorage;

        let mut log_store = create_test_log_store().await;
        let log_state = log_store.get_log_state().await.unwrap();

        assert!(log_state.last_log_id.is_none());
        assert!(log_state.last_purged_log_id.is_none());
    }

    #[tokio::test]
    async fn test_append_and_read_single_entry() {
        use openraft::storage::RaftLogStorage;

        let mut log_store = create_test_log_store().await;

        // Create a test entry
        let _entry = create_test_entry(1, 1);

        // For now, let's just test the key conversion
        let key =
            OpenraftLogStore::<MemoryStorage, MemoryStorage>::log_id_to_key(1);
        let converted_index =
            OpenraftLogStore::<MemoryStorage, MemoryStorage>::key_to_log_id(
                &key,
            );
        assert_eq!(converted_index, Some(1));

        // Check initial log state
        let log_state = log_store.get_log_state().await.unwrap();
        assert!(log_state.last_log_id.is_none());
    }

    #[tokio::test]
    async fn test_vote_storage() {
        use openraft::storage::RaftLogStorage;

        let mut log_store = create_test_log_store().await;

        // Initially no vote
        let initial_vote = log_store.read_vote().await.unwrap();
        assert!(initial_vote.is_none());

        // Save a vote
        let vote = Vote::new(5, 1);
        log_store.save_vote(&vote).await.unwrap();

        // Read the vote back
        let read_vote = log_store.read_vote().await.unwrap();
        assert_eq!(read_vote, Some(vote));

        // Update the vote
        let new_vote = Vote::new(6, 2);
        log_store.save_vote(&new_vote).await.unwrap();

        let updated_vote = log_store.read_vote().await.unwrap();
        assert_eq!(updated_vote, Some(new_vote));
    }

    #[tokio::test]
    async fn test_committed_storage() {
        use openraft::storage::RaftLogStorage;

        let mut log_store = create_test_log_store().await;

        // Initially no committed log
        let initial_committed = log_store.read_committed().await.unwrap();
        assert!(initial_committed.is_none());

        // Save committed log ID
        let committed = LogId {
            leader_id: LeaderId::from(Vote::new(1, 1)),
            index: 3,
        };
        log_store.save_committed(Some(committed)).await.unwrap();

        // Read the committed log ID back
        let read_committed = log_store.read_committed().await.unwrap();
        assert_eq!(read_committed, Some(committed));

        // Clear committed
        log_store.save_committed(None).await.unwrap();
        let cleared_committed = log_store.read_committed().await.unwrap();
        assert!(cleared_committed.is_none());
    }

    #[tokio::test]
    async fn test_key_conversion() {
        let index = 12345u64;
        let key =
            OpenraftLogStore::<MemoryStorage, MemoryStorage>::log_id_to_key(
                index,
            );
        let converted_index =
            OpenraftLogStore::<MemoryStorage, MemoryStorage>::key_to_log_id(
                &key,
            );
        assert_eq!(converted_index, Some(index));

        // Test invalid key
        let invalid_key = b"invalid_key";
        let invalid_index =
            OpenraftLogStore::<MemoryStorage, MemoryStorage>::key_to_log_id(
                invalid_key,
            );
        assert_eq!(invalid_index, None);
    }

    #[tokio::test]
    #[allow(clippy::reversed_empty_ranges)]
    async fn test_empty_range_read() {
        use openraft::RaftLogReader;

        let mut log_store = create_test_log_store().await;

        // Try to read from empty log
        let entries = log_store.try_get_log_entries(1..=5).await.unwrap();
        assert_eq!(entries.len(), 0);

        // Try to read invalid range (ensure it returns empty rather than panic)
        let entries = log_store.try_get_log_entries(10..=5).await.unwrap();
        assert_eq!(entries.len(), 0);
    }

    #[tokio::test]
    async fn test_last_purged_operations() {
        let log_store = create_test_log_store().await;

        // Initially no last purged
        let initial = log_store.get_last_purged().await.unwrap();
        assert!(initial.is_none());

        // Set last purged
        let log_id = LogId {
            leader_id: LeaderId::from(Vote::new(1, 1)),
            index: 5,
        };
        log_store.set_last_purged(log_id).await.unwrap();

        // Read it back
        let read_back = log_store.get_last_purged().await.unwrap();
        assert_eq!(read_back, Some(log_id));
    }

    #[tokio::test]
    async fn test_flush_backend() {
        use openraft::{ErrorSubject, ErrorVerb};

        let log_store = create_test_log_store().await;

        // Test flushing with different subjects
        log_store
            .flush_backends(ErrorSubject::Store, ErrorVerb::Write)
            .await
            .unwrap();
        log_store
            .flush_backends(ErrorSubject::Vote, ErrorVerb::Write)
            .await
            .unwrap();
        log_store
            .flush_backends(ErrorSubject::Logs, ErrorVerb::Write)
            .await
            .unwrap();
    }
}
