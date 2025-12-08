//! ListObjectsHandler - handles list objects operations for the network layer

use std::sync::Arc;

use oprc_grpc::{ListObjectsRequest, ObjectMetaEnvelope};
use oprc_zenoh::util::Handler;
use prost::Message;
use tracing::warn;
use zenoh::{bytes::ZBytes, query::Query};

use crate::granular_trait::ObjectListOptions;
use crate::shard::object_api;
use crate::shard::object_trait::ObjectShard;
use crate::shard::ShardMetadata;

pub(super) struct ListObjectsHandler {
    shard: Option<Arc<dyn ObjectShard>>,
    metadata: ShardMetadata,
}

impl ListObjectsHandler {
    pub fn new(shard: Option<Arc<dyn ObjectShard>>, metadata: ShardMetadata) -> Self {
        Self { shard, metadata }
    }
}

impl Clone for ListObjectsHandler {
    fn clone(&self) -> Self {
        Self {
            shard: self.shard.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

#[async_trait::async_trait]
impl Handler<Query> for ListObjectsHandler {
    async fn handle(&self, query: Query) {
        let id = self.metadata.id;

        let Some(shard) = &self.shard else {
            let _ = query.reply_err(ZBytes::from("shard not attached")).await;
            return;
        };

        // Parse request from payload (or use defaults if no payload)
        let options = if let Some(payload) = query.payload() {
            match ListObjectsRequest::decode(payload.to_bytes().as_ref()) {
                Ok(req) => ObjectListOptions {
                    object_id_prefix: req.object_id_prefix,
                    limit: if req.limit == 0 {
                        100
                    } else {
                        req.limit as usize
                    },
                    cursor: req.cursor,
                },
                Err(e) => {
                    let _ = query
                        .reply_err(ZBytes::from(format!(
                            "failed to decode list request: {}",
                            e
                        )))
                        .await;
                    return;
                }
            }
        } else {
            // No payload: use defaults
            ObjectListOptions::default()
        };

        tracing::debug!(
            "(shard={}) list_objects: prefix={:?} limit={} cursor={}",
            id,
            options.object_id_prefix,
            options.limit,
            options.cursor.is_some()
        );

        match object_api::list_objects(shard.as_ref(), options).await {
            Ok(result) => {
                // Stream results back - for simplicity, send all in one response
                // Each object as separate ObjectMetaEnvelope
                if result.objects.is_empty() {
                    // Send empty marker with cursor if present
                    if let Some(cursor) = result.next_cursor {
                        let envelope = ObjectMetaEnvelope {
                            object_id: String::new(),
                            version: 0,
                            entry_count: 0,
                            next_cursor: Some(cursor),
                        };
                        let payload = ZBytes::from(envelope.encode_to_vec());
                        let _ = query.reply(query.key_expr(), payload).await;
                    }
                    return;
                }

                let last_idx = result.objects.len() - 1;
                for (idx, item) in result.objects.into_iter().enumerate() {
                    let mut envelope = ObjectMetaEnvelope {
                        object_id: item.object_id,
                        version: item.version,
                        entry_count: item.entry_count,
                        next_cursor: None,
                    };
                    if idx == last_idx {
                        envelope.next_cursor = result.next_cursor.clone();
                    }
                    let payload = ZBytes::from(envelope.encode_to_vec());
                    if query.reply(query.key_expr(), payload).await.is_err() {
                        warn!("(shard={}) Failed to send list_objects reply", id);
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = query
                    .reply_err(ZBytes::from(format!("list_objects failed: {:?}", e)))
                    .await;
            }
        }
    }
}
