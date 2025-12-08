//! UnifiedSetterHandler - handles SET operations for the network layer

use std::sync::Arc;

use oprc_grpc::{EmptyResponse, ObjData};
use oprc_zenoh::util::Handler;
use prost::Message;
use tracing::warn;
use zenoh::{
    bytes::ZBytes,
    query::Query,
    sample::{Sample, SampleKind},
};

use super::helpers::{
    extract_identity_from_expr, parse_identity_from_query, storage_key_for_identity,
};
use crate::identity::ObjectIdentity;
use crate::replication::{DeleteOperation, Operation, ReplicationLayer, ShardRequest};
use crate::shard::object_api;
use crate::shard::object_trait::ObjectShard;
use crate::shard::{ObjectData, ShardMetadata};

pub(super) struct UnifiedSetterHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
    max_string_id_len: usize,
    // Optional shard reference for granular operations on string IDs
    shard: Option<Arc<dyn ObjectShard>>,
}

impl<R: ReplicationLayer> UnifiedSetterHandler<R> {
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
}

impl<R: ReplicationLayer> Clone for UnifiedSetterHandler<R> {
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
impl<R: ReplicationLayer + 'static> Handler<Query> for UnifiedSetterHandler<R> {
    async fn handle(&self, query: Query) {
        let id = self.metadata.id;
        if query.payload().is_none() {
            if let Err(e) = query.reply_err(ZBytes::from("payload is required")).await {
                warn!("(shard={}) Failed to reply error for shard: {}", id, e);
            }
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

        let identity_label = match &identity {
            ObjectIdentity::Numeric(oid) => oid.to_string(),
            ObjectIdentity::Str(sid) => sid.clone(),
        };

        tracing::debug!(
            "Unified Shard Network '{}': Handling set query {}",
            id,
            identity_label
        );

        match ObjData::decode(query.payload().unwrap().to_bytes().as_ref()) {
            Ok(obj_data) => {
                let obj_entry = ObjectData::from(obj_data);
                tracing::info!(
                    "UnifiedSetterHandler: decoded object with {} entries",
                    obj_entry.entries.len()
                );
                match identity {
                    ObjectIdentity::Numeric(oid) => {
                        if let Some(shard) = &self.shard {
                            if let Err(e) = object_api::upsert_object(
                                shard.as_ref(),
                                &oid.to_string(),
                                obj_entry.clone(),
                            )
                            .await
                            {
                                let _ = query
                                    .reply_err(ZBytes::from(format!("upsert failed: {:?}", e)))
                                    .await;
                                return;
                            }
                            let payload = ZBytes::from(EmptyResponse::default().encode_to_vec());
                            let _ = query.reply(query.key_expr(), payload).await;
                        } else {
                            let _ = query
                                .reply_err(ZBytes::from("shard not attached for upsert"))
                                .await;
                        }
                    }
                    ObjectIdentity::Str(ref sid) => {
                        if let Some(shard) = &self.shard {
                            if let Err(e) =
                                object_api::upsert_object(shard.as_ref(), sid, obj_entry.clone())
                                    .await
                            {
                                let _ = query
                                    .reply_err(ZBytes::from(format!("upsert failed: {:?}", e)))
                                    .await;
                                return;
                            }
                            let payload = ZBytes::from(EmptyResponse::default().encode_to_vec());
                            let _ = query.reply(query.key_expr(), payload).await;
                        } else {
                            let _ = query
                                .reply_err(ZBytes::from("shard not attached for upsert"))
                                .await;
                        }
                    }
                }
            }
            Err(e) => {
                warn!("Failed to decode object data: {}", e);
                let _ = query
                    .reply_err(ZBytes::from("failed to decode payload"))
                    .await;
            }
        }
    }
}

#[async_trait::async_trait]
impl<R: ReplicationLayer + 'static> Handler<Sample> for UnifiedSetterHandler<R> {
    async fn handle(&self, sample: Sample) {
        let id = self.metadata.id;
        match sample.kind() {
            SampleKind::Put => {
                let expr = sample.key_expr().as_str();
                let Some(identity) = extract_identity_from_expr(expr, self.max_string_id_len)
                else {
                    tracing::debug!(
                        "(shard={}) Failed to extract object identity from sample key '{}'",
                        id,
                        sample.key_expr()
                    );
                    return;
                };
                let _identity_label = match &identity {
                    ObjectIdentity::Numeric(oid) => oid.to_string(),
                    ObjectIdentity::Str(sid) => sid.clone(),
                };
                match ObjData::decode(sample.payload().to_bytes().as_ref()) {
                    Ok(obj_data) => {
                        let obj_entry = ObjectData::from(obj_data);
                        match identity {
                            ObjectIdentity::Numeric(oid) => {
                                if let Some(shard) = &self.shard {
                                    if let Err(e) = object_api::upsert_object(
                                        shard.as_ref(),
                                        &oid.to_string(),
                                        obj_entry.clone(),
                                    )
                                    .await
                                    {
                                        warn!("upsert failed: {:?}", e);
                                    }
                                } else {
                                    warn!("shard not attached for upsert (numeric)");
                                }
                            }
                            ObjectIdentity::Str(ref sid) => {
                                if let Some(shard) = &self.shard {
                                    if let Err(e) = object_api::upsert_object(
                                        shard.as_ref(),
                                        sid,
                                        obj_entry.clone(),
                                    )
                                    .await
                                    {
                                        warn!("upsert failed: {:?}", e);
                                    }
                                } else {
                                    warn!("shard not attached for upsert (string)");
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to decode object data from sample: {}", e);
                    }
                }
            }
            SampleKind::Delete => {
                let expr = sample.key_expr().as_str();
                let Some(identity) = extract_identity_from_expr(expr, self.max_string_id_len)
                else {
                    tracing::debug!(
                        "(shard={}) Failed to extract object identity from delete sample key '{}'",
                        id,
                        sample.key_expr()
                    );
                    return;
                };
                let identity_label = match &identity {
                    ObjectIdentity::Numeric(oid) => oid.to_string(),
                    ObjectIdentity::Str(sid) => sid.clone(),
                };
                match identity {
                    ObjectIdentity::Numeric(_) => {
                        // Legacy blob delete for numeric IDs
                        let operation = Operation::Delete(DeleteOperation {
                            key: storage_key_for_identity(&identity),
                        });
                        let request =
                            ShardRequest::from_operation(operation, self.metadata.id);
                        if let Err(e) = self.replication.replicate_write(request).await {
                            warn!(
                                "Failed to delete object {} for shard {} via replication: {}",
                                identity_label, id, e
                            );
                        }
                    }
                    ObjectIdentity::Str(sid) => {
                        // Granular delete via shard when available
                        if let Some(shard) = &self.shard {
                            if let Err(e) = shard.delete_object_by_str_id(&sid).await {
                                warn!(
                                    "Failed to granular-delete object '{}' for shard {}: {:?}",
                                    sid, id, e
                                );
                            }
                        } else {
                            warn!(
                                "Shard not attached; cannot delete string-id object '{}'",
                                sid
                            );
                        }
                    }
                }
            }
        }
    }
}
