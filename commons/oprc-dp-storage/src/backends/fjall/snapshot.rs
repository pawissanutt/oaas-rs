use async_trait::async_trait;
use fjall::Readable;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_stream::Stream;

use crate::{
    StorageError, StorageValue,
    snapshot::{Snapshot, SnapshotCapableStorage},
};

use super::backend::FjallStorage;

/// Fjall snapshot data - uses Arc around the native Fjall snapshot for cloneability
/// This is a zero-copy snapshot that references the LSM-tree state at a specific point in time
pub type FjallSnapshotData = Arc<fjall::Snapshot>;

#[async_trait]
impl SnapshotCapableStorage for FjallStorage {
    type SnapshotData = FjallSnapshotData;

    async fn create_snapshot(
        &self,
    ) -> Result<Snapshot<Self::SnapshotData>, StorageError> {
        // Create a native Fjall snapshot - this is zero-copy and captures
        // the current state of the LSM-tree without copying any data
        let fjall_snapshot = self.database().snapshot();

        // Wrap in Arc for cloneability
        let snapshot_data = Arc::new(fjall_snapshot);

        // Get snapshot metadata efficiently
        let entry_count = snapshot_data
            .len(self.keyspace())
            .map_err(FjallStorage::convert_snapshot_error)?
            as u64;

        let total_size_bytes = if entry_count > 0 {
            // Estimate size by sampling first and last entries
            let mut estimated_size = 0u64;
            if let Some(guard) = snapshot_data.first_key_value(self.keyspace())
            {
                let (first_key, first_value) = guard
                    .into_inner()
                    .map_err(FjallStorage::convert_snapshot_error)?;
                estimated_size += first_key.as_ref().len() as u64
                    + first_value.as_ref().len() as u64;
            }
            if let Some(guard) = snapshot_data.last_key_value(self.keyspace()) {
                let (last_key, last_value) = guard
                    .into_inner()
                    .map_err(FjallStorage::convert_snapshot_error)?;
                estimated_size += last_key.as_ref().len() as u64
                    + last_value.as_ref().len() as u64;
            }
            // Rough estimation: average entry size * entry count
            if estimated_size > 0 {
                (estimated_size / 2) * entry_count
            } else {
                0
            }
        } else {
            0
        };

        let snapshot_id = format!(
            "fjall_snapshot_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );

        Ok(Snapshot {
            snapshot_id,
            created_at: std::time::SystemTime::now(),
            sequence_number: 0, // Fjall manages its own sequence numbers internally
            snapshot_data,
            entry_count,
            total_size_bytes,
            compression: crate::CompressionType::None,
        })
    }

    async fn restore_from_snapshot(
        &self,
        snapshot: &Snapshot<Self::SnapshotData>,
    ) -> Result<(), StorageError> {
        // Clear existing data first by collecting all keys and removing them
        let all_keys: Vec<Vec<u8>> = self
            .keyspace()
            .iter()
            .map(|guard| {
                let (k, _) =
                    guard.into_inner().map_err(FjallStorage::convert_error)?;
                Ok(k.as_ref().to_vec())
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        for key in all_keys {
            self.keyspace()
                .remove(&key)
                .map_err(FjallStorage::convert_error)?;
        }

        // Restore from snapshot using the native Fjall snapshot iterator
        for guard in snapshot.snapshot_data.iter(self.keyspace()) {
            let (key, value) = guard
                .into_inner()
                .map_err(FjallStorage::convert_snapshot_error)?;
            self.keyspace()
                .insert(key.as_ref(), value.as_ref())
                .map_err(FjallStorage::convert_error)?;
        }

        Ok(())
    }

    async fn latest_snapshot(
        &self,
    ) -> Result<Option<Snapshot<Self::SnapshotData>>, StorageError> {
        // Fjall doesn't maintain snapshot history, so we create a new one
        let snapshot = self.create_snapshot().await?;
        Ok(Some(snapshot))
    }

    async fn estimate_snapshot_size(
        &self,
        snapshot_data: &Self::SnapshotData,
    ) -> Result<u64, StorageError> {
        // For a native Fjall snapshot, we can iterate efficiently
        let mut size = 0u64;

        for guard in snapshot_data.iter(self.keyspace()) {
            let (key, value) = guard
                .into_inner()
                .map_err(FjallStorage::convert_snapshot_error)?;
            size += key.as_ref().len() as u64 + value.as_ref().len() as u64;
        }

        Ok(size)
    }

    async fn create_kv_snapshot_stream(
        &self,
        snapshot: &Snapshot<Self::SnapshotData>,
    ) -> Result<
        Box<
            dyn Stream<
                    Item = Result<(StorageValue, StorageValue), StorageError>,
                > + Send
                + Unpin,
        >,
        StorageError,
    > {
        // Create a stream from the native Fjall snapshot iterator
        let stream =
            FjallSnapshotStream::new(&snapshot.snapshot_data, self.keyspace())?;
        Ok(Box::new(stream))
    }

    async fn install_kv_snapshot_from_stream<S>(
        &self,
        mut stream: S,
    ) -> Result<(), StorageError>
    where
        S: Stream<Item = Result<(StorageValue, StorageValue), StorageError>>
            + Send
            + Unpin,
    {
        use tokio_stream::StreamExt;

        // Clear existing data first
        let all_keys: Vec<Vec<u8>> = self
            .keyspace()
            .iter()
            .map(|guard| {
                let (k, _) =
                    guard.into_inner().map_err(FjallStorage::convert_error)?;
                Ok(k.as_ref().to_vec())
            })
            .collect::<Result<Vec<_>, StorageError>>()?;

        for key in all_keys {
            self.keyspace()
                .remove(&key)
                .map_err(FjallStorage::convert_error)?;
        }

        // Install from stream
        while let Some(result) = stream.next().await {
            let (key, value) = result?;
            let key_bytes: Vec<u8> = key.into_vec();
            let value_bytes: Vec<u8> = value.into_vec();

            self.keyspace()
                .insert(&key_bytes, &value_bytes)
                .map_err(FjallStorage::convert_error)?;
        }

        Ok(())
    }
}
/// Fjall snapshot stream that yields key-value pairs from a native Fjall snapshot
/// This provides a zero-copy streaming interface over the LSM-tree snapshot
pub struct FjallSnapshotStream {
    // Store the items as an iterator over the collected results
    // We need to collect because async traits and Fjall's sync iterators don't mix well
    items:
        std::vec::IntoIter<Result<(StorageValue, StorageValue), StorageError>>,
}

impl FjallSnapshotStream {
    pub fn new(
        snapshot: &FjallSnapshotData,
        keyspace: &Arc<fjall::Keyspace>,
    ) -> Result<Self, StorageError> {
        // Collect all items from the native Fjall snapshot iterator
        // This works directly with the Arc-wrapped Fjall snapshot
        let items: Result<Vec<_>, _> = snapshot
            .iter(keyspace)
            .map(|guard| {
                let (k, v) = guard
                    .into_inner()
                    .map_err(FjallStorage::convert_snapshot_error)?;
                Ok::<(StorageValue, StorageValue), StorageError>((
                    StorageValue::from_slice(k.as_ref()),
                    StorageValue::from_slice(v.as_ref()),
                ))
            })
            .collect();

        let items = items?.into_iter().map(Ok).collect::<Vec<_>>();

        Ok(Self {
            items: items.into_iter(),
        })
    }
}

impl Stream for FjallSnapshotStream {
    type Item = Result<(StorageValue, StorageValue), StorageError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        match self.items.next() {
            Some(item) => Poll::Ready(Some(item)),
            None => Poll::Ready(None),
        }
    }
}

// Make the stream unpin for easier use
impl Unpin for FjallSnapshotStream {}
