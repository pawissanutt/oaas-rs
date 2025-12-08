//! UnifiedGetterHandler - handles GET operations for the network layer

use std::sync::Arc;

use oprc_grpc::ValueResponse;
use oprc_zenoh::util::Handler;
use oprc_dp_storage::StorageValue;
use prost::Message;
use tracing::warn;
use zenoh::{bytes::ZBytes, query::Query};

use super::helpers::{parse_entry_request, parse_identity_from_query};
use crate::granular_key::{ObjectMetadata, build_entry_key, build_metadata_key};
use crate::identity::{ObjectIdentity, normalize_entry_key};
use crate::replication::{
    Operation, ReadOperation, ReplicationLayer, ResponseStatus, ShardRequest,
};
use crate::shard::object_api;
use crate::shard::object_trait::ObjectShard;
use crate::shard::{ObjectVal, ShardMetadata};

pub(super) struct UnifiedGetterHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
    max_string_id_len: usize,
    // Optional shard reference for granular reconstruction
    shard: Option<Arc<dyn ObjectShard>>,
}

impl<R: ReplicationLayer> UnifiedGetterHandler<R> {
    pub fn new(
        replication: Arc<R>,
        metadata: ShardMetadata,
        max_string_id_len: usize,
        shard: Option<Arc<dyn ObjectShard>>,
    ) -> Self {
        Self {
            replication,
            metadata,
            max_string_id_len,
            shard,
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

    async fn handle_entry_query(
        &self,
        query: Query,
        identity: ObjectIdentity,
        raw_entry_key: String,
    ) {
        let id = self.metadata.id;
        let normalized_id = match identity {
            ObjectIdentity::Str(sid) => sid,
            ObjectIdentity::Numeric(_) => {
                if let Err(e) = query
                    .reply_err(ZBytes::from("granular storage requires string object ids"))
                    .await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard {}: {}",
                        id, self.metadata.id, e
                    );
                }
                return;
            }
        };

        let entry_key = match normalize_entry_key(&raw_entry_key, self.max_string_id_len) {
            Ok(k) => k,
            Err(e) => {
                if let Err(err) = query
                    .reply_err(ZBytes::from(format!(
                        "invalid entry key '{}': {}",
                        raw_entry_key, e
                    )))
                    .await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard {}: {}",
                        id, self.metadata.id, err
                    );
                }
                return;
            }
        };

        let entry_key_vec = build_entry_key(&normalized_id, &entry_key);
        let entry_bytes = match self
            .replicate_read_value(StorageValue::from(entry_key_vec))
            .await
        {
            Ok(bytes) => bytes,
            Err(err_msg) => {
                if let Err(e) = query.reply_err(ZBytes::from(err_msg.clone())).await {
                    warn!(
                        "(shard={}) Failed to reply error for shard {}: {}",
                        id, self.metadata.id, e
                    );
                }
                return;
            }
        };

        let Some(bytes) = entry_bytes else {
            if let Err(e) = query.reply_err(ZBytes::from("entry not found")).await {
                warn!(
                    "(shard={}) Failed to reply error for shard {}: {}",
                    id, self.metadata.id, e
                );
            }
            return;
        };

        let (value, _): (ObjectVal, _) = match bincode::serde::decode_from_slice(
            bytes.as_slice(),
            bincode::config::standard(),
        ) {
            Ok(res) => res,
            Err(e) => {
                if let Err(err) = query
                    .reply_err(ZBytes::from(format!(
                        "failed to deserialize entry: {}",
                        e
                    )))
                    .await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard {}: {}",
                        id, self.metadata.id, err
                    );
                }
                return;
            }
        };

        let metadata_bytes = match self
            .replicate_read_value(StorageValue::from(build_metadata_key(&normalized_id)))
            .await
        {
            Ok(bytes) => bytes,
            Err(err_msg) => {
                if let Err(e) = query.reply_err(ZBytes::from(err_msg.clone())).await {
                    warn!(
                        "(shard={}) Failed to reply error for shard {}: {}",
                        id, self.metadata.id, e
                    );
                }
                return;
            }
        };

        let version = match metadata_bytes {
            Some(bytes) => match ObjectMetadata::from_bytes(&bytes) {
                Some(meta) => meta.object_version,
                None => {
                    if let Err(e) = query
                        .reply_err(ZBytes::from("failed to deserialize metadata"))
                        .await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard {}: {}",
                            id, self.metadata.id, e
                        );
                    }
                    return;
                }
            },
            None => 0,
        };

        let response = ValueResponse {
            value: Some(value.into_val()),
            object_version: Some(version),
            key: Some(entry_key),
            deleted: None,
        };

        let payload = ZBytes::from(response.encode_to_vec());
        if let Err(e) = query.reply(query.key_expr(), payload).await {
            warn!(
                "(shard={}) Failed to reply for shard {}: {}",
                id, self.metadata.id, e
            );
        }
    }
}

impl<R: ReplicationLayer> Clone for UnifiedGetterHandler<R> {
    fn clone(&self) -> Self {
        Self {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
            max_string_id_len: self.max_string_id_len,
            shard: self.shard.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<R: ReplicationLayer + 'static> Handler<Query> for UnifiedGetterHandler<R> {
    async fn handle(&self, query: Query) {
        let id = self.metadata.id;
        if let Some((identity, entry_key)) =
            parse_entry_request(query.key_expr().as_str(), self.max_string_id_len)
        {
            self.handle_entry_query(query, identity, entry_key).await;
            return;
        }
        if query.payload().is_some() {
            tracing::debug!(
                "(shard={}) Ignoring get query with payload for key '{}'",
                id,
                query.key_expr()
            );
            return;
        }
        let identity = match parse_identity_from_query(id, &query, self.max_string_id_len) {
            Some(identity) => identity,
            None => {
                if let Err(e) = query.reply_err(ZBytes::from("invalid object id")).await {
                    warn!("(shard={}) Failed to reply error for shard: {}", id, e);
                }
                return;
            }
        };

        let _identity_label = match &identity {
            ObjectIdentity::Numeric(oid) => oid.to_string(),
            ObjectIdentity::Str(sid) => sid.clone(),
        };

        match identity {
            ObjectIdentity::Numeric(oid) => {
                // Treat numeric IDs as string for unified granular reconstruction
                if let Some(shard) = &self.shard {
                    match object_api::get_object(shard.as_ref(), &oid.to_string()).await {
                        Ok(Some(obj)) => {
                            let payload = ZBytes::from(obj.to_data().encode_to_vec());
                            let _ = query.reply(query.key_expr(), payload).await;
                        }
                        Ok(None) => {
                            let _ = query.reply_err(ZBytes::from("object not found")).await;
                        }
                        Err(e) => {
                            let _ = query
                                .reply_err(ZBytes::from(format!("get failed: {:?}", e)))
                                .await;
                        }
                    }
                } else {
                    let _ = query
                        .reply_err(ZBytes::from("shard not attached for granular get"))
                        .await;
                }
            }
            ObjectIdentity::Str(sid) => {
                // Unified granular reconstruction via object_api
                if let Some(shard) = &self.shard {
                    match object_api::get_object(shard.as_ref(), &sid).await {
                        Ok(Some(obj)) => {
                            let payload = ZBytes::from(obj.to_data().encode_to_vec());
                            let _ = query.reply(query.key_expr(), payload).await;
                        }
                        Ok(None) => {
                            let _ = query.reply_err(ZBytes::from("object not found")).await;
                        }
                        Err(e) => {
                            let _ = query
                                .reply_err(ZBytes::from(format!("get failed: {:?}", e)))
                                .await;
                        }
                    }
                } else {
                    let _ = query
                        .reply_err(ZBytes::from("shard not attached for granular get"))
                        .await;
                }
            }
        }
    }
}
