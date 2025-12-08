//! UnifiedBatchSetHandler - handles batch SET operations for the network layer

use std::sync::Arc;

use oprc_dp_storage::StorageValue;
use oprc_grpc::{BatchSetValuesRequest, EmptyResponse};
use oprc_zenoh::util::Handler;
use prost::Message;
use tracing::warn;
use zenoh::{bytes::ZBytes, query::Query};

use super::helpers::parse_batch_set_identity;
use crate::granular_key::{ObjectMetadata, build_entry_key, build_metadata_key};
use crate::identity::{ObjectIdentity, normalize_entry_key};
use crate::replication::{
    DeleteOperation, Operation, ReadOperation, ReplicationLayer, ResponseStatus, ShardRequest,
    WriteOperation,
};
use crate::shard::{ObjectVal, ShardMetadata};

pub(super) struct UnifiedBatchSetHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
    max_string_id_len: usize,
}

impl<R: ReplicationLayer> UnifiedBatchSetHandler<R> {
    pub fn new(replication: Arc<R>, metadata: ShardMetadata, max_string_id_len: usize) -> Self {
        Self {
            replication,
            metadata,
            max_string_id_len,
        }
    }

    async fn replicate_write_operation(&self, operation: Operation) -> Result<(), String> {
        let request = ShardRequest::from_operation(operation, self.metadata.id);
        match self.replication.replicate_write(request).await {
            Ok(response) => match response.status {
                ResponseStatus::Applied => Ok(()),
                ResponseStatus::NotLeader { leader_hint } => {
                    Err(format!("Not leader, hint: {:?}", leader_hint))
                }
                ResponseStatus::Failed(reason) => Err(format!("Write failed: {}", reason)),
                other => Err(format!("Unexpected response status: {:?}", other)),
            },
            Err(e) => Err(format!("replication error: {}", e)),
        }
    }

    async fn replicate_read_value(&self, key: StorageValue) -> Result<Option<Vec<u8>>, String> {
        let request = ShardRequest::from_operation(
            Operation::Read(ReadOperation { key }),
            self.metadata.id,
        );
        match self.replication.replicate_read(request).await {
            Ok(response) => match response.status {
                ResponseStatus::Applied => Ok(response.data.map(|d| d.to_vec())),
                ResponseStatus::NotLeader { leader_hint } => {
                    Err(format!("Not leader, hint: {:?}", leader_hint))
                }
                ResponseStatus::Failed(reason) => Err(format!("Read failed: {}", reason)),
                other => Err(format!("Unexpected response status: {:?}", other)),
            },
            Err(e) => Err(format!("replication error: {}", e)),
        }
    }
}

impl<R: ReplicationLayer> Clone for UnifiedBatchSetHandler<R> {
    fn clone(&self) -> Self {
        Self {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
            max_string_id_len: self.max_string_id_len,
        }
    }
}

#[async_trait::async_trait]
impl<R: ReplicationLayer + 'static> Handler<Query> for UnifiedBatchSetHandler<R> {
    async fn handle(&self, query: Query) {
        let id = self.metadata.id;
        let payload = match query.payload() {
            Some(payload) => payload,
            None => {
                if let Err(e) = query.reply_err(ZBytes::from("payload is required")).await {
                    warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                }
                return;
            }
        };

        let identity =
            match parse_batch_set_identity(query.key_expr().as_str(), self.max_string_id_len) {
                Some(identity) => identity,
                None => {
                    if let Err(e) = query.reply_err(ZBytes::from("invalid object id")).await {
                        warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                    }
                    return;
                }
            };

        let normalized_id = match identity {
            ObjectIdentity::Str(sid) => sid,
            ObjectIdentity::Numeric(_) => {
                if let Err(e) = query
                    .reply_err(ZBytes::from("granular storage requires string object ids"))
                    .await
                {
                    warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                }
                return;
            }
        };

        let request = match BatchSetValuesRequest::decode(payload.to_bytes().as_ref()) {
            Ok(req) => req,
            Err(e) => {
                if let Err(err) = query
                    .reply_err(ZBytes::from(format!(
                        "failed to decode batch request: {}",
                        e
                    )))
                    .await
                {
                    warn!("(shard={}) Failed to reply error for shard: {}", id, err);
                }
                return;
            }
        };

        let mut set_entries: Vec<(String, ObjectVal)> = Vec::with_capacity(request.values.len());
        for (key, value) in &request.values {
            match normalize_entry_key(key, self.max_string_id_len) {
                Ok(norm) => set_entries.push((norm, ObjectVal::from(value.clone()))),
                Err(e) => {
                    if let Err(err) = query
                        .reply_err(ZBytes::from(format!("invalid entry key '{}': {}", key, e)))
                        .await
                    {
                        warn!("(shard={}) Failed to reply error for shard: {}", id, err);
                    }
                    return;
                }
            }
        }

        let mut delete_keys: Vec<String> = Vec::with_capacity(request.delete_keys.len());
        for key in &request.delete_keys {
            match normalize_entry_key(key, self.max_string_id_len) {
                Ok(norm) => delete_keys.push(norm),
                Err(e) => {
                    if let Err(err) = query
                        .reply_err(ZBytes::from(format!("invalid delete key '{}': {}", key, e)))
                        .await
                    {
                        warn!("(shard={}) Failed to reply error for shard: {}", id, err);
                    }
                    return;
                }
            }
        }

        let metadata_key_vec = build_metadata_key(&normalized_id);
        let metadata_bytes = match self
            .replicate_read_value(StorageValue::from(metadata_key_vec.clone()))
            .await
        {
            Ok(bytes) => bytes,
            Err(err_msg) => {
                if let Err(e) = query.reply_err(ZBytes::from(err_msg.clone())).await {
                    warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                }
                return;
            }
        };

        let mut metadata = match metadata_bytes {
            Some(bytes) => match ObjectMetadata::from_bytes(&bytes) {
                Some(meta) => meta,
                None => {
                    if let Err(e) = query
                        .reply_err(ZBytes::from("failed to deserialize metadata"))
                        .await
                    {
                        warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                    }
                    return;
                }
            },
            None => ObjectMetadata::default(),
        };

        let expected_version = request.expected_object_version;
        let mutated = !set_entries.is_empty() || !delete_keys.is_empty();
        if mutated {
            if let Some(expected) = expected_version {
                if metadata.object_version != expected {
                    if let Err(e) = query
                        .reply_err(ZBytes::from(format!(
                            "version mismatch: expected {}, got {}",
                            expected, metadata.object_version
                        )))
                        .await
                    {
                        warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                    }
                    return;
                }
            }
        }

        let mut ops: Vec<Operation> = Vec::new();
        if !set_entries.is_empty() {
            for (key, value) in &set_entries {
                let entry_key = build_entry_key(&normalized_id, key);
                let value_bytes =
                    match bincode::serde::encode_to_vec(&value, bincode::config::standard()) {
                        Ok(bytes) => bytes,
                        Err(e) => {
                            if let Err(err) = query
                                .reply_err(ZBytes::from(format!(
                                    "failed to serialize entry '{}': {}",
                                    key, e
                                )))
                                .await
                            {
                                warn!("(shard={}) Failed to reply error for shard: {}", id, err);
                            }
                            return;
                        }
                    };
                ops.push(Operation::Write(WriteOperation {
                    key: StorageValue::from(entry_key),
                    value: StorageValue::from(value_bytes),
                    ..Default::default()
                }));
            }
        }

        if !delete_keys.is_empty() {
            for key in &delete_keys {
                let entry_key_vec = build_entry_key(&normalized_id, key);
                ops.push(Operation::Delete(DeleteOperation {
                    key: StorageValue::from(entry_key_vec),
                }));
            }
        }

        if mutated {
            let mut bumps = 0;
            if !set_entries.is_empty() {
                bumps += 1;
            }
            if !delete_keys.is_empty() {
                bumps += 1;
            }
            for _ in 0..bumps {
                metadata.increment_version();
            }
            ops.push(Operation::Write(WriteOperation {
                key: StorageValue::from(metadata_key_vec),
                value: StorageValue::from(metadata.to_bytes()),
                ..Default::default()
            }));
        }

        if !ops.is_empty() {
            if let Err(err_msg) = self.replicate_write_operation(Operation::Batch(ops)).await {
                if let Err(e) = query.reply_err(ZBytes::from(err_msg.clone())).await {
                    warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                }
                return;
            }
        }

        let payload = ZBytes::from(EmptyResponse::default().encode_to_vec());
        if let Err(e) = query.reply(query.key_expr(), payload).await {
            warn!("(shard={}) Failed to reply for shard: {}", id, e);
        }
    }
}
