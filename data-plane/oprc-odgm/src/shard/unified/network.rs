use std::sync::Arc;

use flume::Receiver;
use oprc_grpc::{EmptyResponse, ObjData};
use oprc_zenoh::util::{
    Handler, ManagedConfig, declare_managed_queryable,
    declare_managed_subscriber,
};
use prost::Message;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use zenoh::{
    bytes::ZBytes,
    pubsub::Subscriber,
    query::{Query, Queryable},
    sample::{Sample, SampleKind},
};

use super::traits::ShardMetadata;
use crate::replication::{
    DeleteOperation, Operation, ReadOperation, ReplicationLayer,
    ResponseStatus, ShardRequest, WriteOperation,
};
use crate::shard::ObjectEntry;
use oprc_dp_storage::StorageValue;

/// Modern unified shard network interface
/// Works with ReplicationLayer instead of direct shard access for better consistency
pub struct UnifiedShardNetwork<R: ReplicationLayer> {
    z_session: zenoh::Session,
    replication: Arc<R>,
    metadata: ShardMetadata,
    token: CancellationToken,
    prefix: String,
    set_subscriber: Option<Subscriber<Receiver<Sample>>>,
    set_queryable: Option<Queryable<Receiver<Query>>>,
    get_queryable: Option<Queryable<Receiver<Query>>>,
}

impl<R: ReplicationLayer + 'static> UnifiedShardNetwork<R> {
    pub fn new(
        z_session: zenoh::Session,
        replication: Arc<R>,
        metadata: ShardMetadata,
        prefix: String,
    ) -> Self {
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            replication,
            metadata,
            token,
            prefix,
            set_subscriber: None,
            set_queryable: None,
            get_queryable: None,
        }
    }

    pub async fn start(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!(
            "Starting network for {} with prefix '{}'",
            self.metadata.id,
            self.prefix
        );
        self.token = CancellationToken::new();
        self.start_set_subscriber().await?;
        self.start_set_queryable().await?;
        self.start_get_queryable().await?;
        Ok(())
    }

    #[tracing::instrument(skip(self), fields(id=%self.metadata.id))]
    pub async fn stop(&mut self) {
        if let Some(sub) = self.set_subscriber.take() {
            if let Err(e) = sub.undeclare().await {
                tracing::warn!("Failed to undeclare subscriber: {}", e);
            };
        }
        if let Some(q) = self.set_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
        if let Some(q) = self.get_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
    }

    pub fn is_running(&self) -> bool {
        !self.token.is_cancelled()
    }

    async fn start_set_subscriber(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*", self.prefix);
        let handler = UnifiedSetterHandler {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
        };
        let config = ManagedConfig::new(key, 16, 65536);
        let s = declare_managed_subscriber(&self.z_session, config, handler)
            .await?;
        self.set_subscriber = Some(s);
        Ok(())
    }

    async fn start_set_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/set/*", self.prefix);
        let handler = UnifiedSetterHandler {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
        };
        let config = ManagedConfig::new(key, 16, 65536);
        let q =
            declare_managed_queryable(&self.z_session, config, handler).await?;
        self.set_queryable = Some(q);
        Ok(())
    }

    async fn start_get_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/get/*", self.prefix);
        let handler = UnifiedGetterHandler {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
        };
        let config = ManagedConfig::new(key, 16, 65536);
        let q =
            declare_managed_queryable(&self.z_session, config, handler).await?;
        self.get_queryable = Some(q);
        Ok(())
    }
}

#[inline]
async fn parse_oid_from_query(
    shard_id: u64,
    query: &zenoh::query::Query,
) -> Option<u64> {
    let expr = query.key_expr().as_str();
    if let Some(oid_str) = expr.split('/').last() {
        match oid_str.parse::<u64>() {
            Ok(oid) => return Some(oid),
            Err(e) => {
                tracing::debug!(
                    "(shard={}) Failed to parse OID '{}' from key '{}': {}",
                    shard_id,
                    oid_str,
                    expr,
                    e
                );
            }
        }
    }

    // Fallback: try to extract from query selector
    let selector = query.selector();
    let parameters = selector.parameters();
    if let Some(oid_param) = parameters.get("oid") {
        if let Ok(oid) = oid_param.parse::<u64>() {
            return Some(oid);
        }
    }

    tracing::debug!(
        "(shard={}) Failed to get object ID from keys: {}",
        shard_id,
        query.key_expr()
    );
    None
}

struct UnifiedSetterHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
}

impl<R: ReplicationLayer> Clone for UnifiedSetterHandler<R> {
    fn clone(&self) -> Self {
        Self {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<R: ReplicationLayer + 'static> Handler<Query> for UnifiedSetterHandler<R> {
    async fn handle(&self, query: Query) {
        let id = self.metadata.id;
        if query.payload().is_none() {
            if let Err(e) =
                query.reply_err(ZBytes::from("payload is required")).await
            {
                warn!("(shard={}) Failed to reply error for shard: {}", id, e);
            }
            return;
        }
        let parsed_id = parse_oid_from_query(id, &query).await;
        if let Some(oid) = parsed_id {
            tracing::debug!(
                "Unified Shard Network '{}': Handling set query {}",
                id,
                oid
            );
            match ObjData::decode(query.payload().unwrap().to_bytes().as_ref())
            {
                Ok(obj_data) => {
                    let obj_entry = ObjectEntry::from(obj_data);

                    // Serialize the object entry to storage value
                    let storage_value = match bincode::serde::encode_to_vec(
                        &obj_entry,
                        bincode::config::standard(),
                    ) {
                        Ok(bytes) => StorageValue::from(bytes),
                        Err(e) => {
                            warn!("Failed to serialize object entry: {}", e);
                            if let Err(e) = query
                                .reply_err(ZBytes::from(
                                    "failed to serialize object",
                                ))
                                .await
                            {
                                warn!(
                                    "(shard={}) Failed to reply error for shard: {}",
                                    id, e
                                );
                            }
                            return;
                        }
                    };

                    // Create replication write request
                    let operation = Operation::Write(WriteOperation {
                        key: StorageValue::from(oid.to_be_bytes().to_vec()),
                        value: storage_value,
                        ..Default::default()
                    });

                    let request = ShardRequest::from_operation(
                        operation,
                        self.metadata.id,
                    );

                    // Execute via replication layer
                    match self.replication.replicate_write(request).await {
                        Ok(response) => match response.status {
                            ResponseStatus::Applied => {
                                let payload =
                                    EmptyResponse::default().encode_to_vec();
                                let payload = ZBytes::from(payload);
                                if let Err(e) =
                                    query.reply(query.key_expr(), payload).await
                                {
                                    warn!(
                                        "(shard={}) Failed to reply for shard: {}",
                                        id, e
                                    );
                                }
                            }
                            ResponseStatus::NotLeader { leader_hint } => {
                                let error_msg = format!(
                                    "Not leader, hint: {:?}",
                                    leader_hint
                                );
                                if let Err(e) = query
                                    .reply_err(ZBytes::from(error_msg))
                                    .await
                                {
                                    warn!(
                                        "(shard={}) Failed to reply error for shard: {}",
                                        id, e
                                    );
                                }
                            }
                            ResponseStatus::Failed(reason) => {
                                warn!("Write operation failed: {}", reason);
                                if let Err(e) = query
                                    .reply_err(ZBytes::from(format!(
                                        "Write failed: {}",
                                        reason
                                    )))
                                    .await
                                {
                                    warn!(
                                        "(shard={}) Failed to reply error for shard: {}",
                                        id, e
                                    );
                                }
                            }
                            _ => {
                                warn!(
                                    "Unexpected response status: {:?}",
                                    response.status
                                );
                                if let Err(e) = query
                                    .reply_err(ZBytes::from(
                                        "Unexpected response status",
                                    ))
                                    .await
                                {
                                    warn!(
                                        "(shard={}) Failed to reply error for shard: {}",
                                        id, e
                                    );
                                }
                            }
                        },
                        Err(e) => {
                            warn!(
                                "Failed to execute write operation via replication: {}",
                                e
                            );
                            if let Err(e) = query
                                .reply_err(ZBytes::from("replication error"))
                                .await
                            {
                                warn!(
                                    "(shard={}) Failed to reply error for shard: {}",
                                    id, e
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to decode object data: {}", e);
                    if let Err(e) = query
                        .reply_err(ZBytes::from("failed to decode payload"))
                        .await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, e
                        );
                    }
                }
            }
        }
    }
}

#[async_trait::async_trait]
impl<R: ReplicationLayer + 'static> Handler<Sample>
    for UnifiedSetterHandler<R>
{
    async fn handle(&self, sample: Sample) {
        let id = self.metadata.id;
        match sample.kind() {
            SampleKind::Put => {
                let oid = if let Some(oid_str) =
                    sample.key_expr().as_str().split('/').last()
                {
                    match oid_str.parse::<u64>() {
                        Ok(oid) => oid,
                        Err(e) => {
                            tracing::debug!(
                                "(shard={}) Failed to parse OID from sample key '{}': {}",
                                id,
                                sample.key_expr(),
                                e
                            );
                            return;
                        }
                    }
                } else {
                    tracing::debug!(
                        "(shard={}) Failed to extract OID from sample key '{}'",
                        id,
                        sample.key_expr()
                    );
                    return;
                };

                match ObjData::decode(sample.payload().to_bytes().as_ref()) {
                    Ok(obj_data) => {
                        let obj_entry = ObjectEntry::from(obj_data);

                        // Serialize the object entry to storage value
                        let storage_value = match bincode::serde::encode_to_vec(
                            &obj_entry,
                            bincode::config::standard(),
                        ) {
                            Ok(bytes) => StorageValue::from(bytes),
                            Err(e) => {
                                warn!(
                                    "Failed to serialize object entry: {}",
                                    e
                                );
                                return;
                            }
                        };

                        // Create replication write request
                        let operation = Operation::Write(WriteOperation {
                            key: StorageValue::from(oid.to_be_bytes().to_vec()),
                            value: storage_value,
                            ..Default::default()
                        });

                        let request = ShardRequest::from_operation(
                            operation,
                            self.metadata.id,
                        );
                        // Execute via replication layer
                        if let Err(e) =
                            self.replication.replicate_write(request).await
                        {
                            warn!(
                                "Failed to set object {} for shard {} via replication: {}",
                                oid, id, e
                            );
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to decode object data from sample: {}",
                            e
                        );
                    }
                }
            }
            SampleKind::Delete => {
                let oid = if let Some(oid_str) =
                    sample.key_expr().as_str().split('/').last()
                {
                    match oid_str.parse::<u64>() {
                        Ok(oid) => oid,
                        Err(e) => {
                            tracing::debug!(
                                "(shard={}) Failed to parse OID from delete sample key '{}': {}",
                                id,
                                sample.key_expr(),
                                e
                            );
                            return;
                        }
                    }
                } else {
                    tracing::debug!(
                        "(shard={}) Failed to extract OID from delete sample key '{}'",
                        id,
                        sample.key_expr()
                    );
                    return;
                };

                // Create replication delete request
                let operation = Operation::Delete(DeleteOperation {
                    key: StorageValue::from(oid.to_be_bytes().to_vec()),
                });

                let request =
                    ShardRequest::from_operation(operation, self.metadata.id);

                // Execute via replication layer
                if let Err(e) = self.replication.replicate_write(request).await
                {
                    warn!(
                        "Failed to delete object {} for shard {} via replication: {}",
                        oid, id, e
                    );
                }
            }
        }
    }
}

struct UnifiedGetterHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
}

impl<R: ReplicationLayer> Clone for UnifiedGetterHandler<R> {
    fn clone(&self) -> Self {
        Self {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

#[async_trait::async_trait]
impl<R: ReplicationLayer + 'static> Handler<Query> for UnifiedGetterHandler<R> {
    async fn handle(&self, query: Query) {
        let id = self.metadata.id;
        let parsed_id = parse_oid_from_query(id, &query).await;
        if let Some(oid) = parsed_id {
            // Create replication read request
            let operation = Operation::Read(ReadOperation {
                key: StorageValue::from(oid.to_be_bytes().to_vec()),
            });

            let request =
                ShardRequest::from_operation(operation, self.metadata.id);

            // Execute via replication layer
            match self.replication.replicate_read(request).await {
                Ok(response) => match response.status {
                    ResponseStatus::Applied => {
                        if let Some(data) = response.data {
                            // Deserialize the storage value back to ObjectEntry
                            match bincode::serde::decode_from_slice::<
                                ObjectEntry,
                                _,
                            >(
                                data.as_slice(), bincode::config::standard()
                            ) {
                                Ok((entry, _)) => {
                                    let obj_data = entry.to_data();
                                    let payload = obj_data.encode_to_vec();
                                    let payload = ZBytes::from(payload);
                                    if let Err(e) = query
                                        .reply(query.key_expr(), payload)
                                        .await
                                    {
                                        warn!(
                                            "(shard={}) Failed to reply for shard: {}",
                                            id, e
                                        );
                                    }
                                }
                                Err(e) => {
                                    warn!(
                                        "Failed to deserialize object entry: {}",
                                        e
                                    );
                                    if let Err(e) = query
                                        .reply_err(ZBytes::from(
                                            "failed to deserialize object",
                                        ))
                                        .await
                                    {
                                        warn!(
                                            "(shard={}) Failed to reply error for shard: {}",
                                            id, e
                                        );
                                    }
                                }
                            }
                        } else {
                            if let Err(e) = query
                                .reply_err(ZBytes::from("object not found"))
                                .await
                            {
                                warn!(
                                    "(shard={}) Failed to reply error for shard: {}",
                                    id, e
                                );
                            }
                        }
                    }
                    ResponseStatus::NotLeader { leader_hint } => {
                        let error_msg =
                            format!("Not leader, hint: {:?}", leader_hint);
                        if let Err(e) =
                            query.reply_err(ZBytes::from(error_msg)).await
                        {
                            warn!(
                                "(shard={}) Failed to reply error for shard: {}",
                                id, e
                            );
                        }
                    }
                    ResponseStatus::Failed(reason) => {
                        warn!("Read operation failed: {}", reason);
                        if let Err(e) = query
                            .reply_err(ZBytes::from(format!(
                                "Read failed: {}",
                                reason
                            )))
                            .await
                        {
                            warn!(
                                "(shard={}) Failed to reply error for shard: {}",
                                id, e
                            );
                        }
                    }
                    _ => {
                        warn!(
                            "Unexpected response status: {:?}",
                            response.status
                        );
                        if let Err(e) = query
                            .reply_err(ZBytes::from(
                                "Unexpected response status",
                            ))
                            .await
                        {
                            warn!(
                                "(shard={}) Failed to reply error for shard: {}",
                                id, e
                            );
                        }
                    }
                },
                Err(e) => {
                    warn!(
                        "Failed to execute read operation via replication: {}",
                        e
                    );
                    if let Err(e) =
                        query.reply_err(ZBytes::from("replication error")).await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, e
                        );
                    }
                }
            }
        }
    }
}
