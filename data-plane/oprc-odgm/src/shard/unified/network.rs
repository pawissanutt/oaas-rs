use std::sync::Arc;

use super::object_api; // unified granular API
use super::object_trait::ObjectShard;
use flume::Receiver;
use oprc_grpc::{
    BatchSetValuesRequest, EmptyResponse, ListObjectsRequest, ObjData,
    ObjectMetaEnvelope, ValueResponse,
};
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
use crate::granular_key::{
    ObjectMetadata, build_entry_key, build_metadata_key,
};
use crate::granular_trait::ObjectListOptions;
use crate::identity::{
    ObjectIdentity, normalize_entry_key, normalize_object_id,
};
use crate::replication::{
    DeleteOperation, Operation, ReadOperation, ReplicationLayer,
    ResponseStatus, ShardRequest, WriteOperation,
};
use crate::shard::{ObjectData, ObjectVal};
use oprc_dp_storage::StorageValue;

/// Modern unified shard network interface
/// Works with ReplicationLayer instead of direct shard access for better consistency
pub struct UnifiedShardNetwork<R: ReplicationLayer> {
    z_session: zenoh::Session,
    replication: Arc<R>,
    metadata: ShardMetadata,
    token: CancellationToken,
    prefix: String,
    max_string_id_len: usize,
    set_subscriber: Option<Subscriber<Receiver<Sample>>>,
    set_queryable: Option<Queryable<Receiver<Query>>>,
    get_queryable: Option<Queryable<Receiver<Query>>>,
    batch_set_queryable: Option<Queryable<Receiver<Query>>>,
    list_objects_queryable: Option<Queryable<Receiver<Query>>>,
    // New: reference to shard for granular reconstruction & mutations
    shard: Option<Arc<dyn ObjectShard>>, // attached after shard creation
}

impl<R: ReplicationLayer + 'static> UnifiedShardNetwork<R> {
    pub fn new(
        z_session: zenoh::Session,
        replication: Arc<R>,
        metadata: ShardMetadata,
        prefix: String,
        max_string_id_len: usize,
    ) -> Self {
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            replication,
            metadata,
            token,
            prefix,
            max_string_id_len,
            set_subscriber: None,
            set_queryable: None,
            get_queryable: None,
            batch_set_queryable: None,
            list_objects_queryable: None,
            shard: None,
        }
    }

    /// Attach the concrete shard after it has been constructed and wrapped in Arc.
    /// This enables GET/SET handlers to use unified granular APIs.
    pub fn attach_shard(&mut self, shard: Arc<dyn ObjectShard>) {
        self.shard = Some(shard);
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
        self.start_batch_set_queryable().await?;
        self.start_list_objects_queryable().await?;
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
        if let Some(q) = self.batch_set_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!("Failed to undeclare queryable: {}", e);
            };
        }
        if let Some(q) = self.list_objects_queryable.take() {
            if let Err(e) = q.undeclare().await {
                tracing::warn!(
                    "Failed to undeclare list_objects queryable: {}",
                    e
                );
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
            max_string_id_len: self.max_string_id_len,
            shard: self.shard.clone(),
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
        let key = format!("{}/*/set", self.prefix);
        let handler = UnifiedSetterHandler {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
            max_string_id_len: self.max_string_id_len,
            shard: self.shard.clone(),
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
        let key = format!("{}/**", self.prefix);
        let handler = UnifiedGetterHandler {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
            max_string_id_len: self.max_string_id_len,
            shard: self.shard.clone(),
        };
        let config = ManagedConfig::new(key, 16, 65536);
        let q =
            declare_managed_queryable(&self.z_session, config, handler).await?;
        self.get_queryable = Some(q);
        Ok(())
    }

    async fn start_batch_set_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("{}/*/batch-set", self.prefix);
        let handler = UnifiedBatchSetHandler {
            replication: self.replication.clone(),
            metadata: self.metadata.clone(),
            max_string_id_len: self.max_string_id_len,
        };
        let config = ManagedConfig::new(key, 16, 65536);
        let q =
            declare_managed_queryable(&self.z_session, config, handler).await?;
        self.batch_set_queryable = Some(q);
        Ok(())
    }

    async fn start_list_objects_queryable(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Key pattern: oprc/<cls>/<pid>/list-objects
        let key = format!("{}/list-objects", self.prefix);
        let handler = ListObjectsHandler {
            shard: self.shard.clone(),
            metadata: self.metadata.clone(),
        };
        let config = ManagedConfig::new(key, 16, 65536);
        let q =
            declare_managed_queryable(&self.z_session, config, handler).await?;
        self.list_objects_queryable = Some(q);
        Ok(())
    }
}

fn split_path_segments(expr: &str) -> Vec<&str> {
    expr.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

fn parse_object_identity_segment(
    segment: &str,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    if let Ok(num) = segment.parse::<u64>() {
        return Some(ObjectIdentity::Numeric(num));
    }
    match normalize_object_id(segment, max_string_id_len) {
        Ok(norm) => Some(ObjectIdentity::Str(norm)),
        Err(e) => {
            tracing::debug!(
                "Failed to normalize string object id '{}': {}",
                segment,
                e
            );
            None
        }
    }
}

fn extract_identity_from_expr(
    expr: &str,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    let segments = split_path_segments(expr);
    let objects_pos = segments.iter().rposition(|seg| *seg == "objects")?;
    let mut idx = objects_pos + 1;
    if idx >= segments.len() {
        return None;
    }
    let mut candidate = segments[idx];
    if matches!(candidate, "get" | "set" | "batch-set") {
        idx += 1;
        if idx >= segments.len() {
            return None;
        }
        candidate = segments[idx];
    }
    parse_object_identity_segment(candidate, max_string_id_len)
}

fn parse_entry_request(
    expr: &str,
    max_string_id_len: usize,
) -> Option<(ObjectIdentity, String)> {
    let segments = split_path_segments(expr);
    let objects_pos = segments.iter().rposition(|seg| *seg == "objects")?;
    let obj_idx = objects_pos + 1;
    if obj_idx + 2 >= segments.len() {
        return None;
    }
    if segments[obj_idx + 1] != "entries" {
        return None;
    }
    let identity =
        parse_object_identity_segment(segments[obj_idx], max_string_id_len)?;
    let entry_key = segments[obj_idx + 2].to_string();
    Some((identity, entry_key))
}

fn parse_batch_set_identity(
    expr: &str,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    let segments = split_path_segments(expr);
    let objects_pos = segments.iter().rposition(|seg| *seg == "objects")?;
    let obj_idx = objects_pos + 1;
    if obj_idx + 1 >= segments.len() {
        return None;
    }
    if segments[obj_idx + 1] != "batch-set" {
        return None;
    }
    parse_object_identity_segment(segments[obj_idx], max_string_id_len)
}

fn parse_identity_from_query(
    shard_id: u64,
    query: &zenoh::query::Query,
    max_string_id_len: usize,
) -> Option<ObjectIdentity> {
    if let Some(identity) =
        extract_identity_from_expr(query.key_expr().as_str(), max_string_id_len)
    {
        return Some(identity);
    }

    let selector = query.selector();
    let parameters = selector.parameters();
    if let Some(oid_param) = parameters.get("oid") {
        if let Ok(oid) = oid_param.parse::<u64>() {
            return Some(ObjectIdentity::Numeric(oid));
        }
    }
    if let Some(oid_str) = parameters.get("oid_str") {
        match normalize_object_id(oid_str, max_string_id_len) {
            Ok(norm) => return Some(ObjectIdentity::Str(norm)),
            Err(e) => {
                tracing::debug!(
                    "(shard={}) Failed to normalize object_id_str '{}': {}",
                    shard_id,
                    oid_str,
                    e
                );
            }
        }
    }

    tracing::debug!(
        "(shard={}) Failed to parse object identity from key '{}'",
        shard_id,
        query.key_expr()
    );
    None
}

// Legacy storage key helper for numeric identity; string variant unused in granular path.
fn storage_key_for_identity(identity: &ObjectIdentity) -> StorageValue {
    match identity {
        ObjectIdentity::Numeric(oid) => {
            StorageValue::from(oid.to_be_bytes().to_vec())
        }
        ObjectIdentity::Str(sid) => StorageValue::from(sid.as_bytes()),
    }
}

struct UnifiedSetterHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
    max_string_id_len: usize,
    // Optional shard reference for granular operations on string IDs
    shard: Option<Arc<dyn ObjectShard>>,
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

impl<R: ReplicationLayer> UnifiedGetterHandler<R> {
    async fn replicate_read_value(
        &self,
        key: StorageValue,
    ) -> Result<Option<Vec<u8>>, String> {
        let request = ShardRequest::from_operation(
            Operation::Read(ReadOperation { key }),
            self.metadata.id,
        );
        match self.replication.replicate_read(request).await {
            Ok(response) => match response.status {
                ResponseStatus::Applied => {
                    Ok(response.data.map(|d| d.to_vec()))
                }
                ResponseStatus::NotLeader { leader_hint } => {
                    Err(format!("Not leader, hint: {:?}", leader_hint))
                }
                ResponseStatus::Failed(reason) => {
                    Err(format!("Read failed: {}", reason))
                }
                other => {
                    Err(format!("Unexpected response status: {:?}", other))
                }
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
                    .reply_err(ZBytes::from(
                        "granular storage requires string object ids",
                    ))
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

        let entry_key =
            match normalize_entry_key(&raw_entry_key, self.max_string_id_len) {
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
                if let Err(e) =
                    query.reply_err(ZBytes::from(err_msg.clone())).await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard {}: {}",
                        id, self.metadata.id, e
                    );
                }
                return;
            }
        };

        let Some(bytes) = entry_bytes else {
            if let Err(e) =
                query.reply_err(ZBytes::from("entry not found")).await
            {
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
            .replicate_read_value(StorageValue::from(build_metadata_key(
                &normalized_id,
            )))
            .await
        {
            Ok(bytes) => bytes,
            Err(err_msg) => {
                if let Err(e) =
                    query.reply_err(ZBytes::from(err_msg.clone())).await
                {
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
                        .reply_err(ZBytes::from(
                            "failed to deserialize metadata",
                        ))
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

impl<R: ReplicationLayer> UnifiedSetterHandler<R> {}

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
        let identity =
            match parse_identity_from_query(id, &query, self.max_string_id_len)
            {
                Some(identity) => identity,
                None => {
                    if let Err(e) =
                        query.reply_err(ZBytes::from("invalid object id")).await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, e
                        );
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
                                    .reply_err(ZBytes::from(format!(
                                        "upsert failed: {:?}",
                                        e
                                    )))
                                    .await;
                                return;
                            }
                            let payload = ZBytes::from(
                                EmptyResponse::default().encode_to_vec(),
                            );
                            let _ =
                                query.reply(query.key_expr(), payload).await;
                        } else {
                            let _ = query
                                .reply_err(ZBytes::from(
                                    "shard not attached for upsert",
                                ))
                                .await;
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
                                let _ = query
                                    .reply_err(ZBytes::from(format!(
                                        "upsert failed: {:?}",
                                        e
                                    )))
                                    .await;
                                return;
                            }
                            let payload = ZBytes::from(
                                EmptyResponse::default().encode_to_vec(),
                            );
                            let _ =
                                query.reply(query.key_expr(), payload).await;
                        } else {
                            let _ = query
                                .reply_err(ZBytes::from(
                                    "shard not attached for upsert",
                                ))
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
impl<R: ReplicationLayer + 'static> Handler<Sample>
    for UnifiedSetterHandler<R>
{
    async fn handle(&self, sample: Sample) {
        let id = self.metadata.id;
        match sample.kind() {
            SampleKind::Put => {
                let expr = sample.key_expr().as_str();
                let Some(identity) =
                    extract_identity_from_expr(expr, self.max_string_id_len)
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
                                    warn!(
                                        "shard not attached for upsert (numeric)"
                                    );
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
                                    warn!(
                                        "shard not attached for upsert (string)"
                                    );
                                }
                            }
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
                let expr = sample.key_expr().as_str();
                let Some(identity) =
                    extract_identity_from_expr(expr, self.max_string_id_len)
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
                        let request = ShardRequest::from_operation(
                            operation,
                            self.metadata.id,
                        );
                        if let Err(e) =
                            self.replication.replicate_write(request).await
                        {
                            warn!(
                                "Failed to delete object {} for shard {} via replication: {}",
                                identity_label, id, e
                            );
                        }
                    }
                    ObjectIdentity::Str(sid) => {
                        // Granular delete via shard when available
                        if let Some(shard) = &self.shard {
                            if let Err(e) =
                                shard.delete_object_by_str_id(&sid).await
                            {
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

struct UnifiedGetterHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
    max_string_id_len: usize,
    // Optional shard reference for granular reconstruction
    shard: Option<Arc<dyn ObjectShard>>,
}

struct UnifiedBatchSetHandler<R: ReplicationLayer> {
    replication: Arc<R>,
    metadata: ShardMetadata,
    max_string_id_len: usize,
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

impl<R: ReplicationLayer> UnifiedBatchSetHandler<R> {
    async fn replicate_write_operation(
        &self,
        operation: Operation,
    ) -> Result<(), String> {
        let request = ShardRequest::from_operation(operation, self.metadata.id);
        match self.replication.replicate_write(request).await {
            Ok(response) => match response.status {
                ResponseStatus::Applied => Ok(()),
                ResponseStatus::NotLeader { leader_hint } => {
                    Err(format!("Not leader, hint: {:?}", leader_hint))
                }
                ResponseStatus::Failed(reason) => {
                    Err(format!("Write failed: {}", reason))
                }
                other => {
                    Err(format!("Unexpected response status: {:?}", other))
                }
            },
            Err(e) => Err(format!("replication error: {}", e)),
        }
    }

    async fn replicate_read_value(
        &self,
        key: StorageValue,
    ) -> Result<Option<Vec<u8>>, String> {
        let request = ShardRequest::from_operation(
            Operation::Read(ReadOperation { key }),
            self.metadata.id,
        );
        match self.replication.replicate_read(request).await {
            Ok(response) => match response.status {
                ResponseStatus::Applied => {
                    Ok(response.data.map(|d| d.to_vec()))
                }
                ResponseStatus::NotLeader { leader_hint } => {
                    Err(format!("Not leader, hint: {:?}", leader_hint))
                }
                ResponseStatus::Failed(reason) => {
                    Err(format!("Read failed: {}", reason))
                }
                other => {
                    Err(format!("Unexpected response status: {:?}", other))
                }
            },
            Err(e) => Err(format!("replication error: {}", e)),
        }
    }
}

#[async_trait::async_trait]
impl<R: ReplicationLayer + 'static> Handler<Query>
    for UnifiedBatchSetHandler<R>
{
    async fn handle(&self, query: Query) {
        let id = self.metadata.id;
        let payload = match query.payload() {
            Some(payload) => payload,
            None => {
                if let Err(e) =
                    query.reply_err(ZBytes::from("payload is required")).await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard: {}",
                        id, e
                    );
                }
                return;
            }
        };

        let identity = match parse_batch_set_identity(
            query.key_expr().as_str(),
            self.max_string_id_len,
        ) {
            Some(identity) => identity,
            None => {
                if let Err(e) =
                    query.reply_err(ZBytes::from("invalid object id")).await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard: {}",
                        id, e
                    );
                }
                return;
            }
        };

        let normalized_id = match identity {
            ObjectIdentity::Str(sid) => sid,
            ObjectIdentity::Numeric(_) => {
                if let Err(e) = query
                    .reply_err(ZBytes::from(
                        "granular storage requires string object ids",
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

        let request =
            match BatchSetValuesRequest::decode(payload.to_bytes().as_ref()) {
                Ok(req) => req,
                Err(e) => {
                    if let Err(err) = query
                        .reply_err(ZBytes::from(format!(
                            "failed to decode batch request: {}",
                            e
                        )))
                        .await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, err
                        );
                    }
                    return;
                }
            };

        let mut set_entries: Vec<(String, ObjectVal)> =
            Vec::with_capacity(request.values.len());
        for (key, value) in &request.values {
            match normalize_entry_key(key, self.max_string_id_len) {
                Ok(norm) => {
                    set_entries.push((norm, ObjectVal::from(value.clone())))
                }
                Err(e) => {
                    if let Err(err) = query
                        .reply_err(ZBytes::from(format!(
                            "invalid entry key '{}': {}",
                            key, e
                        )))
                        .await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, err
                        );
                    }
                    return;
                }
            }
        }

        let mut delete_keys: Vec<String> =
            Vec::with_capacity(request.delete_keys.len());
        for key in &request.delete_keys {
            match normalize_entry_key(key, self.max_string_id_len) {
                Ok(norm) => delete_keys.push(norm),
                Err(e) => {
                    if let Err(err) = query
                        .reply_err(ZBytes::from(format!(
                            "invalid delete key '{}': {}",
                            key, e
                        )))
                        .await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, err
                        );
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
                if let Err(e) =
                    query.reply_err(ZBytes::from(err_msg.clone())).await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard: {}",
                        id, e
                    );
                }
                return;
            }
        };

        let mut metadata = match metadata_bytes {
            Some(bytes) => match ObjectMetadata::from_bytes(&bytes) {
                Some(meta) => meta,
                None => {
                    if let Err(e) = query
                        .reply_err(ZBytes::from(
                            "failed to deserialize metadata",
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
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, e
                        );
                    }
                    return;
                }
            }
        }

        let mut ops: Vec<Operation> = Vec::new();
        if !set_entries.is_empty() {
            for (key, value) in &set_entries {
                let entry_key = build_entry_key(&normalized_id, &key);
                let value_bytes = match bincode::serde::encode_to_vec(
                    &value,
                    bincode::config::standard(),
                ) {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        if let Err(err) = query
                            .reply_err(ZBytes::from(format!(
                                "failed to serialize entry '{}': {}",
                                key, e
                            )))
                            .await
                        {
                            warn!(
                                "(shard={}) Failed to reply error for shard: {}",
                                id, err
                            );
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
            if let Err(err_msg) =
                self.replicate_write_operation(Operation::Batch(ops)).await
            {
                if let Err(e) =
                    query.reply_err(ZBytes::from(err_msg.clone())).await
                {
                    warn!(
                        "(shard={}) Failed to reply error for shard: {}",
                        id, e
                    );
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
        if let Some((identity, entry_key)) = parse_entry_request(
            query.key_expr().as_str(),
            self.max_string_id_len,
        ) {
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
        let identity =
            match parse_identity_from_query(id, &query, self.max_string_id_len)
            {
                Some(identity) => identity,
                None => {
                    if let Err(e) =
                        query.reply_err(ZBytes::from("invalid object id")).await
                    {
                        warn!(
                            "(shard={}) Failed to reply error for shard: {}",
                            id, e
                        );
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
                    match object_api::get_object(
                        shard.as_ref(),
                        &oid.to_string(),
                    )
                    .await
                    {
                        Ok(Some(obj)) => {
                            let payload =
                                ZBytes::from(obj.to_data().encode_to_vec());
                            let _ =
                                query.reply(query.key_expr(), payload).await;
                        }
                        Ok(None) => {
                            let _ = query
                                .reply_err(ZBytes::from("object not found"))
                                .await;
                        }
                        Err(e) => {
                            let _ = query
                                .reply_err(ZBytes::from(format!(
                                    "get failed: {:?}",
                                    e
                                )))
                                .await;
                        }
                    }
                } else {
                    let _ = query
                        .reply_err(ZBytes::from(
                            "shard not attached for granular get",
                        ))
                        .await;
                }
            }
            ObjectIdentity::Str(sid) => {
                // Unified granular reconstruction via object_api
                if let Some(shard) = &self.shard {
                    match object_api::get_object(shard.as_ref(), &sid).await {
                        Ok(Some(obj)) => {
                            let payload =
                                ZBytes::from(obj.to_data().encode_to_vec());
                            let _ =
                                query.reply(query.key_expr(), payload).await;
                        }
                        Ok(None) => {
                            let _ = query
                                .reply_err(ZBytes::from("object not found"))
                                .await;
                        }
                        Err(e) => {
                            let _ = query
                                .reply_err(ZBytes::from(format!(
                                    "get failed: {:?}",
                                    e
                                )))
                                .await;
                        }
                    }
                } else {
                    let _ = query
                        .reply_err(ZBytes::from(
                            "shard not attached for granular get",
                        ))
                        .await;
                }
            }
        }
    }
}

// ============================================================
// ListObjects Handler - Lists objects in partition with pagination
// ============================================================

struct ListObjectsHandler {
    shard: Option<Arc<dyn ObjectShard>>,
    metadata: ShardMetadata,
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
                        warn!(
                            "(shard={}) Failed to send list_objects reply",
                            id
                        );
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = query
                    .reply_err(ZBytes::from(format!(
                        "list_objects failed: {:?}",
                        e
                    )))
                    .await;
            }
        }
    }
}
