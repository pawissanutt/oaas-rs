use futures::Stream;
use oprc_grpc::{
    EmptyResponse, ListObjectsRequest, ObjectMetaEnvelope, ObjectResponse,
    SetKeyRequest, SetObjectRequest, ShardStats, SingleKeyRequest,
    SingleObjectRequest, StatsRequest, StatsResponse, ValueEnvelope,
    ValueResponse, data_service_server::DataService,
};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Response, Status};
use tracing::{debug, trace};

use crate::granular_trait::{EntryListOptions, ObjectListOptions};
use crate::identity::{
    NormalizationError, ObjectIdentity, build_identity, normalize_entry_key,
};
use crate::metrics::{
    incr_entry_mutation, incr_get, incr_set, record_normalize_latency_ms,
};
use crate::shard::unified::object_api; // new unified API for string ID operations
use crate::{
    cluster::ObjectDataGridManager,
    shard::{
        ObjectData, ObjectVal, basic::ObjectError, unified::config::ShardError,
    },
};

pub struct OdgmDataService {
    odgm: Arc<ObjectDataGridManager>,
    max_string_id_len: usize,
    enable_string_entry_keys: bool,
    enable_granular_entry_storage: bool,
    granular_prefetch_limit: usize,
}

impl OdgmDataService {
    pub fn new(
        odgm: Arc<ObjectDataGridManager>,
        max_string_id_len: usize,
        enable_string_entry_keys: bool,
        enable_granular_entry_storage: bool,
        granular_prefetch_limit: usize,
    ) -> Self {
        OdgmDataService {
            odgm,
            max_string_id_len,
            enable_string_entry_keys,
            enable_granular_entry_storage,
            granular_prefetch_limit,
        }
    }

    #[inline]
    fn build_identity_from_string(
        &self,
        object_id: &str,
    ) -> Result<ObjectIdentity, Status> {
        if object_id.is_empty() {
            return Err(Status::invalid_argument("object_id required"));
        }
        let start = std::time::Instant::now();
        let res = build_identity(
            None, // force string path
            Some(object_id),
            self.max_string_id_len,
        )
        .map_err(|e| {
            Status::invalid_argument(format!("invalid identity: {e}"))
        })?;
        let elapsed = start.elapsed();
        record_normalize_latency_ms(elapsed.as_secs_f64() * 1000.0);
        Ok(res)
    }

    #[inline]
    fn ensure_granular_enabled(&self) -> Result<(), Status> {
        Ok(()) // always enabled now
    }

    #[inline]
    fn ensure_string_entry_keys_enabled(&self) -> Result<(), Status> {
        if self.enable_string_entry_keys {
            Ok(())
        } else {
            Err(Status::unimplemented("string entry keys feature disabled"))
        }
    }

    fn normalize_entry_key_or_error(
        &self,
        raw: &str,
    ) -> Result<String, Status> {
        normalize_entry_key(raw, self.max_string_id_len)
            .map_err(|e| Self::map_normalization_error("entry key", e))
    }

    fn resolve_entry_key(&self, key: Option<String>) -> Result<String, Status> {
        if let Some(k) = key {
            self.ensure_string_entry_keys_enabled()?;
            self.normalize_entry_key_or_error(&k)
        } else {
            Err(Status::invalid_argument("key required"))
        }
    }

    fn map_normalization_error(
        kind: &str,
        error: NormalizationError,
    ) -> Status {
        match error {
            NormalizationError::Empty => {
                Status::invalid_argument(format!("{} cannot be empty", kind))
            }
            NormalizationError::Length { len, max } => {
                Status::invalid_argument(format!(
                    "{} length {} exceeds maximum {}",
                    kind, len, max
                ))
            }
            NormalizationError::InvalidChars => Status::invalid_argument(
                format!("{} contains invalid characters", kind),
            ),
            NormalizationError::BothProvided
            | NormalizationError::NoneProvided => {
                Status::invalid_argument(format!("invalid {} parameters", kind))
            }
        }
    }
}

#[tonic::async_trait]
impl DataService for OdgmDataService {
    type ListValuesStream = Pin<
        Box<dyn Stream<Item = Result<ValueEnvelope, Status>> + Send + 'static>,
    >;

    type ListObjectsStream = Pin<
        Box<
            dyn Stream<Item = Result<ObjectMetaEnvelope, Status>>
                + Send
                + 'static,
        >,
    >;

    async fn get(
        &self,
        request: tonic::Request<SingleObjectRequest>,
    ) -> std::result::Result<tonic::Response<ObjectResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        debug!("receive get request: {:?}", key_request);
        let object_id = key_request.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        let entry_opt = match &identity {
            ObjectIdentity::Str(sid) => {
                shard
                    .reconstruct_object_granular(
                        sid,
                        self.granular_prefetch_limit,
                    )
                    .await?
            }
            ObjectIdentity::Numeric(_) => {
                unreachable!("numeric variant should not be produced anymore")
            }
        };
        let variant = "string";
        incr_get(variant);
        if let Some(entry) = entry_opt {
            if let ObjectIdentity::Str(sid) = identity {
                let mut data = entry.to_data();
                data.metadata = Some(oprc_grpc::ObjMeta {
                    cls_id: key_request.cls_id.clone(),
                    partition_id: key_request.partition_id as u32,
                    object_id: Some(sid.clone()),
                });
                return Ok(Response::new(ObjectResponse { obj: Some(data) }));
            }
        }
        Err(Status::not_found("object not found"))
    }

    async fn get_value(
        &self,
        request: tonic::Request<SingleKeyRequest>,
    ) -> std::result::Result<tonic::Response<ValueResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        debug!("receive get_value request: {:?}", key_request);
        let object_id = key_request.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        // Granular-only path for both numeric and string identities
        let normalized_id = match &identity {
            ObjectIdentity::Str(sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => unreachable!(),
        };
        let entry_key = self.resolve_entry_key(key_request.key)?;
        incr_get("string");
        let value_opt =
            shard.get_entry_granular(&normalized_id, &entry_key).await?;
        let metadata = shard.get_metadata_granular(&normalized_id).await?;
        let response = ValueResponse {
            value: value_opt.map(|v| v.into_val()),
            object_version: metadata.map(|m| m.object_version),
            key: Some(entry_key),
            deleted: None,
        };
        return Ok(Response::new(response));
    }

    async fn delete(
        &self,
        request: tonic::Request<SingleObjectRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        debug!("receive delete request: {:?}", key_request);
        let object_id = key_request.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        if let ObjectIdentity::Str(ref sid) = identity {
            shard.delete_object_by_str_id(sid).await?;
        }
        Ok(Response::new(EmptyResponse {}))
    }

    async fn set(
        &self,
        request: tonic::Request<SetObjectRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        if tracing::enabled!(tracing::Level::TRACE) {
            trace!("receive set request: {:?}", key_request);
        } else {
            debug!(
                "receive set request: {} {} {:?}",
                key_request.cls_id,
                key_request.partition_id,
                key_request.object_id
            );
        }
        let object_id = key_request.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        let obj_raw = key_request.object.unwrap();
        if !self.enable_string_entry_keys && !obj_raw.entries.is_empty() {
            return Err(Status::unimplemented(
                "string entry keys feature disabled",
            ));
        }
        let obj = ObjectData::from(obj_raw);
        if let ObjectIdentity::Str(ref sid) = identity {
            object_api::upsert_object(shard.as_ref(), sid, obj)
                .await
                .map_err(|e| Status::internal(format!("upsert failed: {e}")))?;
            incr_set("string");
        }
        Ok(Response::new(EmptyResponse {}))
    }

    async fn set_value(
        &self,
        request: tonic::Request<SetKeyRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        let object_id = key_request.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        if key_request.value.is_none() {
            return Err(Status::invalid_argument("object must not be none"));
        }
        let value_proto = key_request.value.clone().unwrap();

        let normalized_id = match &identity {
            ObjectIdentity::Str(sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => unreachable!(),
        }; // numeric removed
        let entry_key = self.resolve_entry_key(key_request.key)?;
        let oval = ObjectVal::from(value_proto.clone());
        shard
            .set_entry_granular(&normalized_id, &entry_key, oval)
            .await?;
        incr_entry_mutation("string");
        Ok(Response::new(EmptyResponse {}))
    }

    async fn merge(
        &self,
        request: tonic::Request<SetObjectRequest>,
    ) -> Result<tonic::Response<ObjectResponse>, tonic::Status> {
        let key_request = request.into_inner();
        let object_id = key_request.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        if key_request.object.is_some() {
            let raw = key_request.object.unwrap();
            if !self.enable_string_entry_keys && !raw.entries.is_empty() {
                return Err(Status::unimplemented(
                    "string entry keys feature disabled",
                ));
            }
            let new_obj = ObjectData::from(raw);
            let merged_obj = if let ObjectIdentity::Str(ref sid) = identity {
                if let Some(mut existing) =
                    shard.get_object_by_str_id(sid).await?
                {
                    existing.merge(new_obj)?;
                    existing
                } else {
                    new_obj
                }
            } else {
                unreachable!()
            };
            if let ObjectIdentity::Str(ref sid) = identity {
                shard.set_object_by_str_id(sid, merged_obj.clone()).await?;
                let mut data = merged_obj.to_data();
                data.metadata = Some(oprc_grpc::ObjMeta {
                    cls_id: key_request.cls_id.clone(),
                    partition_id: key_request.partition_id as u32,
                    object_id: Some(sid.clone()),
                });
                let resp = oprc_grpc::ObjectResponse { obj: Some(data) };
                Ok(Response::new(resp))
            } else {
                unreachable!()
            }
        } else {
            return Err(Status::invalid_argument("object must not be none"));
        }
    }

    async fn stats(
        &self,
        _request: tonic::Request<StatsRequest>,
    ) -> std::result::Result<tonic::Response<StatsResponse>, tonic::Status>
    {
        let mut stats = Vec::new();

        // Get stats from all shards in the manager
        self.odgm
            .shard_manager
            .shards
            .iter_async(|shard_id, shard| {
                let meta = shard.meta();
                // Get count by using a blocking call within the iter
                if let Ok(l) = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(shard.count_objects())
                }) {
                    stats.push(ShardStats {
                        shard_id: *shard_id,
                        collection: meta.collection.clone(),
                        partition_id: meta.partition_id as u32,
                        count: l as u64,
                    });
                }
                true
            })
            .await;

        Ok(Response::new(StatsResponse { shards: stats }))
    }

    async fn capabilities(
        &self,
        _request: tonic::Request<oprc_grpc::CapabilitiesRequest>,
    ) -> Result<tonic::Response<oprc_grpc::CapabilitiesResponse>, tonic::Status>
    {
        let resp = oprc_grpc::CapabilitiesResponse {
            string_ids: true,
            string_entry_keys: self.enable_string_entry_keys,
            granular_entry_storage: self.enable_granular_entry_storage,
            event_pipeline_v2: std::env::var("ODGM_EVENT_PIPELINE_V2")
                .ok()
                .and_then(|v| v.parse::<bool>().ok())
                .unwrap_or(false),
        };
        Ok(Response::new(resp))
    }

    // Phase A: Granular storage RPCs gated by ODGM_ENABLE_GRANULAR_STORAGE
    // TODO: wire to config flag once implemented

    async fn delete_value(
        &self,
        request: tonic::Request<SingleKeyRequest>,
    ) -> Result<tonic::Response<EmptyResponse>, tonic::Status> {
        self.ensure_granular_enabled()?;
        let req = request.into_inner();
        let object_id = req.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        let normalized_id = match identity {
            ObjectIdentity::Str(ref sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => unreachable!(),
        };

        let entry_key = self.resolve_entry_key(req.key)?;
        shard
            .delete_entry_granular(&normalized_id, &entry_key)
            .await?;
        Ok(Response::new(EmptyResponse {}))
    }

    async fn batch_set_values(
        &self,
        request: tonic::Request<oprc_grpc::BatchSetValuesRequest>,
    ) -> Result<tonic::Response<EmptyResponse>, tonic::Status> {
        self.ensure_granular_enabled()?;
        self.ensure_string_entry_keys_enabled()?;

        let mut req = request.into_inner();
        let object_id = req.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        let normalized_id = match identity {
            ObjectIdentity::Str(ref sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => unreachable!(),
        };

        let expected_version = req.expected_object_version;

        let mut set_map = HashMap::with_capacity(req.values.len());
        for (key, value) in std::mem::take(&mut req.values) {
            let normalized_key = self.normalize_entry_key_or_error(&key)?;
            set_map.insert(normalized_key, ObjectVal::from(value));
        }

        let mut delete_keys = Vec::with_capacity(req.delete_keys.len());
        for key in std::mem::take(&mut req.delete_keys) {
            delete_keys.push(self.normalize_entry_key_or_error(&key)?);
        }

        // Delegate to unified API (string ID only). Numeric IDs remain legacy/unimplemented here.
        object_api::batch_mutate_entries(
            shard.as_ref(),
            &normalized_id,
            set_map,
            delete_keys,
            expected_version,
        )
        .await
        .map_err(Status::from)?;

        Ok(Response::new(EmptyResponse {}))
    }

    async fn list_values(
        &self,
        request: tonic::Request<oprc_grpc::ListValuesRequest>,
    ) -> Result<tonic::Response<Self::ListValuesStream>, tonic::Status> {
        self.ensure_granular_enabled()?;
        self.ensure_string_entry_keys_enabled()?;

        let req = request.into_inner();
        let object_id = req.object_id.as_deref().unwrap_or_default();
        let identity = self.build_identity_from_string(object_id)?;
        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        let normalized_id = match identity {
            ObjectIdentity::Str(ref sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => unreachable!(),
        };

        let limit = if req.limit == 0 {
            EntryListOptions::default().limit
        } else {
            req.limit as usize
        };

        let key_prefix = if let Some(prefix) = req.key_prefix.as_ref() {
            Some(self.normalize_entry_key_or_error(prefix)?)
        } else {
            None
        };

        let options = EntryListOptions {
            key_prefix,
            limit,
            cursor: req.cursor.clone(),
        };

        let (entries, next_cursor, version) =
            object_api::list_entries(shard.as_ref(), &normalized_id, options)
                .await
                .map_err(Status::from)?;
        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            if entries.is_empty() {
                if let Some(cursor) = next_cursor {
                    let envelope = ValueEnvelope {
                        key: String::new(),
                        value: None,
                        version,
                        next_cursor: Some(cursor),
                    };
                    let _ = tx.send(Ok(envelope)).await;
                }
                return;
            }

            let last_index = entries.len() - 1;
            for (idx, (key, value)) in entries.into_iter().enumerate() {
                let mut envelope = ValueEnvelope {
                    key,
                    value: Some(value.into_val()),
                    version,
                    next_cursor: None,
                };
                if idx == last_index {
                    envelope.next_cursor = next_cursor.clone();
                }
                if tx.send(Ok(envelope)).await.is_err() {
                    return;
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::ListValuesStream))
    }

    /// List objects in a partition (metadata only, for browsing/discovery).
    async fn list_objects(
        &self,
        request: tonic::Request<ListObjectsRequest>,
    ) -> Result<tonic::Response<Self::ListObjectsStream>, tonic::Status> {
        let req = request.into_inner();
        debug!(
            "receive list_objects request: cls={} partition={}",
            req.cls_id, req.partition_id
        );

        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        let limit = if req.limit == 0 {
            ObjectListOptions::default().limit
        } else {
            req.limit as usize
        };

        let options = ObjectListOptions {
            object_id_prefix: req.object_id_prefix,
            limit,
            cursor: req.cursor,
        };

        let result = object_api::list_objects(shard.as_ref(), options)
            .await
            .map_err(Status::from)?;

        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            if result.objects.is_empty() {
                // Return empty cursor marker if provided
                if let Some(cursor) = result.next_cursor {
                    let envelope = ObjectMetaEnvelope {
                        object_id: String::new(),
                        version: 0,
                        entry_count: 0,
                        next_cursor: Some(cursor),
                    };
                    let _ = tx.send(Ok(envelope)).await;
                }
                return;
            }

            let last_index = result.objects.len() - 1;
            for (idx, item) in result.objects.into_iter().enumerate() {
                let mut envelope = ObjectMetaEnvelope {
                    object_id: item.object_id,
                    version: item.version,
                    entry_count: item.entry_count,
                    next_cursor: None,
                };
                if idx == last_index {
                    envelope.next_cursor = result.next_cursor.clone();
                }
                if tx.send(Ok(envelope)).await.is_err() {
                    return;
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream) as Self::ListObjectsStream))
    }
}

impl From<ObjectError> for tonic::Status {
    fn from(error: ObjectError) -> Self {
        match error {
            ObjectError::CrdtError(e) => {
                Status::internal(format!("CRDT merge failed: {}", e))
            }
            ObjectError::SerializationError(msg) => Status::invalid_argument(
                format!("Serialization error: {}", msg),
            ),
            ObjectError::InvalidDataFormat(msg) => Status::invalid_argument(
                format!("Invalid data format: {}", msg),
            ),
        }
    }
}

impl From<ShardError> for tonic::Status {
    fn from(error: ShardError) -> Self {
        match error {
            ShardError::StorageError(e) => {
                Status::internal(format!("Storage error: {}", e))
            }
            ShardError::ReplicationError(msg) => {
                Status::internal(format!("Replication error: {}", msg))
            }
            ShardError::SerializationError(msg) => Status::invalid_argument(
                format!("Serialization error: {}", msg),
            ),
            ShardError::ConfigurationError(msg) => Status::invalid_argument(
                format!("Configuration error: {}", msg),
            ),
            ShardError::TransactionError(msg) => {
                Status::aborted(format!("Transaction error: {}", msg))
            }
            ShardError::InvalidKey => {
                Status::invalid_argument("Invalid key format")
            }
            ShardError::NotReady => Status::unavailable("Shard not ready"),
            ShardError::NotLeader => Status::failed_precondition("Not leader"),
            ShardError::NoShardsFound(shard_ids) => {
                Status::not_found(format!("No shards found: {:?}", shard_ids))
            }
            ShardError::OdgmError(e) => {
                Status::internal(format!("ODGM error: {}", e))
            }
            // Phase C: Granular storage errors
            ShardError::InvalidMetadata => {
                Status::internal("Invalid metadata format")
            }
            ShardError::DeserializationError => {
                Status::internal("Failed to deserialize data")
            }
            ShardError::VersionMismatch { expected, actual } => {
                Status::aborted(format!(
                    "Version mismatch: expected {}, got {}",
                    expected, actual
                ))
            }
        }
    }
}
