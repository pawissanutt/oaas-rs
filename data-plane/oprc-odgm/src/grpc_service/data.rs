use futures::Stream;
use oprc_grpc::{
    EmptyResponse, ObjectResponse, SetKeyRequest, SetObjectRequest, ShardStats,
    SingleKeyRequest, SingleObjectRequest, StatsRequest, StatsResponse,
    ValueEnvelope, ValueResponse, data_service_server::DataService,
};
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Response, Status};
use tracing::{debug, trace};

use crate::granular_key::numeric_key_to_string;
use crate::granular_trait::{EntryListOptions, EntryListResult};
use crate::identity::{
    NormalizationError, ObjectIdentity, build_identity, normalize_entry_key,
};
use crate::metrics::{
    incr_entry_mutation, incr_get, incr_set, record_normalize_latency_ms,
};
use crate::{
    cluster::ObjectDataGridManager,
    shard::{
        ObjectData, ObjectVal, basic::ObjectError, unified::config::ShardError,
    },
};

pub struct OdgmDataService {
    odgm: Arc<ObjectDataGridManager>,
    max_string_id_len: usize,
    enable_string_ids: bool,
    enable_string_entry_keys: bool,
    enable_granular_entry_storage: bool,
    granular_prefetch_limit: usize,
}

impl OdgmDataService {
    pub fn new(
        odgm: Arc<ObjectDataGridManager>,
        max_string_id_len: usize,
        enable_string_ids: bool,
        enable_string_entry_keys: bool,
        enable_granular_entry_storage: bool,
        granular_prefetch_limit: usize,
    ) -> Self {
        OdgmDataService {
            odgm,
            max_string_id_len,
            enable_string_ids,
            enable_string_entry_keys,
            enable_granular_entry_storage,
            granular_prefetch_limit,
        }
    }

    #[inline]
    fn build_identity_numeric_first(
        &self,
        numeric: u64,
        string_opt: Option<String>,
    ) -> Result<ObjectIdentity, Status> {
        // If both a non-zero numeric and a string ID are supplied, reject explicitly.
        if string_opt.is_some() && numeric != 0 {
            return Err(Status::invalid_argument(
                "both object_id and object_id_str provided",
            ));
        }
        let numeric_opt = if string_opt.is_none() {
            Some(numeric)
        } else {
            None
        };
        if let Some(ref s) = string_opt {
            if !self.enable_string_ids {
                return Err(Status::unimplemented(
                    "string object ids feature disabled",
                ));
            }
            let start = std::time::Instant::now();
            let res = build_identity(
                numeric_opt,
                Some(s.as_str()),
                self.max_string_id_len,
            )
            .map_err(|e| {
                Status::invalid_argument(format!("invalid identity: {e}"))
            })?;
            let elapsed = start.elapsed();
            record_normalize_latency_ms(elapsed.as_secs_f64() * 1000.0);
            Ok(res)
        } else {
            build_identity(numeric_opt, None, self.max_string_id_len).map_err(
                |e| Status::invalid_argument(format!("invalid identity: {e}")),
            )
        }
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

    fn resolve_entry_key(
        &self,
        numeric_key: u32,
        key_str: Option<String>,
    ) -> Result<(String, &'static str), Status> {
        if let Some(key) = key_str {
            self.ensure_string_entry_keys_enabled()?;
            let normalized = self.normalize_entry_key_or_error(&key)?;
            Ok((normalized, "string"))
        } else {
            Ok((numeric_key_to_string(numeric_key), "numeric"))
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

    async fn get(
        &self,
        request: tonic::Request<SingleObjectRequest>,
    ) -> std::result::Result<tonic::Response<ObjectResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        debug!("receive get request: {:?}", key_request);
        let identity = self.build_identity_numeric_first(
            key_request.object_id,
            key_request.object_id_str.clone(),
        )?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        let entry_opt = match &identity {
            ObjectIdentity::Numeric(oid) => shard.get_object(*oid).await?,
            ObjectIdentity::Str(sid) => {
                shard
                    .reconstruct_object_granular(
                        sid,
                        self.granular_prefetch_limit,
                    )
                    .await?
            }
        };
        let variant = match &identity {
            ObjectIdentity::Numeric(_) => "numeric",
            ObjectIdentity::Str(_) => "string",
        };
        incr_get(variant);
        if let Some(entry) = entry_opt {
            match identity {
                ObjectIdentity::Numeric(_) => {
                    Ok(Response::new(entry.to_resp()))
                }
                ObjectIdentity::Str(sid) => {
                    // Inject metadata with object_id_str
                    let mut data = entry.to_data();
                    data.metadata = Some(oprc_grpc::ObjMeta {
                        cls_id: key_request.cls_id.clone(),
                        partition_id: key_request.partition_id as u32,
                        object_id: 0,
                        object_id_str: Some(sid.clone()),
                    });
                    Ok(Response::new(oprc_grpc::ObjectResponse {
                        obj: Some(data),
                    }))
                }
            }
        } else {
            Err(Status::not_found("not found data"))
        }
    }

    async fn get_value(
        &self,
        request: tonic::Request<SingleKeyRequest>,
    ) -> std::result::Result<tonic::Response<ValueResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        debug!("receive get_value request: {:?}", key_request);
        let identity = self.build_identity_numeric_first(
            key_request.object_id,
            key_request.object_id_str.clone(),
        )?;
        if key_request.key != 0 && key_request.key_str.is_some() {
            return Err(Status::invalid_argument(
                "both key and key_str provided",
            ));
        }
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
            ObjectIdentity::Numeric(oid) => oid.to_string(),
            ObjectIdentity::Str(sid) => sid.clone(),
        };
        let (entry_key, variant) = self
            .resolve_entry_key(key_request.key, key_request.key_str.clone())?;
        incr_get(variant);
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
        // unreachable
    }

    async fn delete(
        &self,
        request: tonic::Request<SingleObjectRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        debug!("receive delete request: {:?}", key_request);
        let identity = self.build_identity_numeric_first(
            key_request.object_id,
            key_request.object_id_str.clone(),
        )?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        match identity {
            ObjectIdentity::Numeric(oid) => shard.delete_object(&oid).await?,
            ObjectIdentity::Str(ref sid) => {
                shard.delete_object_by_str_id(sid).await?
            }
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
                "receive set request: {} {} {}",
                key_request.cls_id,
                key_request.partition_id,
                key_request.object_id
            );
        }
        let identity = self.build_identity_numeric_first(
            key_request.object_id,
            key_request.object_id_str.clone(),
        )?;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        let obj_raw = key_request.object.unwrap();
        if !self.enable_string_entry_keys && !obj_raw.entries_str.is_empty() {
            return Err(Status::unimplemented(
                "string entry keys feature disabled",
            ));
        }
        let obj = ObjectData::from(obj_raw);
        match identity {
            ObjectIdentity::Numeric(oid) => {
                // Preserve legacy upsert behavior for numeric IDs (needed for event update tests).
                let existed: bool = shard.get_object(oid).await?.is_some();
                shard.set_object(oid, obj).await?;
                if !existed {
                    incr_set("numeric");
                }
            }
            ObjectIdentity::Str(ref sid) => {
                if shard.get_object_by_str_id(sid).await?.is_some() {
                    return Err(Status::already_exists(
                        "object already exists",
                    ));
                }
                shard.set_object_by_str_id(sid, obj).await?;
                incr_set("string");
            }
        }
        Ok(Response::new(EmptyResponse {}))
    }

    async fn set_value(
        &self,
        request: tonic::Request<SetKeyRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        let identity = self.build_identity_numeric_first(
            key_request.object_id,
            key_request.object_id_str.clone(),
        )?;
        if key_request.key != 0 && key_request.key_str.is_some() {
            return Err(Status::invalid_argument(
                "both key and key_str provided",
            ));
        }
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
            ObjectIdentity::Numeric(oid) => oid.to_string(),
            ObjectIdentity::Str(sid) => sid.clone(),
        };
        let (entry_key, variant) = self
            .resolve_entry_key(key_request.key, key_request.key_str.clone())?;
        let oval = ObjectVal::from(value_proto.clone());
        shard
            .set_entry_granular(&normalized_id, &entry_key, oval)
            .await?;
        incr_entry_mutation(variant);
        Ok(Response::new(EmptyResponse {}))
    }

    async fn merge(
        &self,
        request: tonic::Request<SetObjectRequest>,
    ) -> Result<tonic::Response<ObjectResponse>, tonic::Status> {
        let key_request = request.into_inner();
        let identity = self.build_identity_numeric_first(
            key_request.object_id,
            key_request.object_id_str.clone(),
        )?;
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
            if !self.enable_string_entry_keys && !raw.entries_str.is_empty() {
                return Err(Status::unimplemented(
                    "string entry keys feature disabled",
                ));
            }
            let new_obj = ObjectData::from(raw);
            let merged_obj = match identity {
                ObjectIdentity::Numeric(oid) => {
                    if let Some(mut existing) = shard.get_object(oid).await? {
                        existing.merge(new_obj)?;
                        existing
                    } else {
                        new_obj
                    }
                }
                ObjectIdentity::Str(ref sid) => {
                    if let Some(mut existing) =
                        shard.get_object_by_str_id(sid).await?
                    {
                        existing.merge(new_obj)?;
                        existing
                    } else {
                        new_obj
                    }
                }
            };
            match identity {
                ObjectIdentity::Numeric(oid) => {
                    shard.set_object(oid, merged_obj.clone()).await?
                }
                ObjectIdentity::Str(ref sid) => {
                    shard.set_object_by_str_id(sid, merged_obj.clone()).await?
                }
            }
            let resp = match identity {
                ObjectIdentity::Numeric(_) => merged_obj.to_resp(),
                ObjectIdentity::Str(ref sid) => {
                    let mut data = merged_obj.to_data();
                    data.metadata = Some(oprc_grpc::ObjMeta {
                        cls_id: key_request.cls_id.clone(),
                        partition_id: key_request.partition_id as u32,
                        object_id: 0,
                        object_id_str: Some(sid.clone()),
                    });
                    oprc_grpc::ObjectResponse { obj: Some(data) }
                }
            };
            Ok(Response::new(resp))
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
            string_ids: self.enable_string_ids,
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
        let identity = self.build_identity_numeric_first(
            req.object_id,
            req.object_id_str.clone(),
        )?;
        if req.key != 0 && req.key_str.is_some() {
            return Err(Status::invalid_argument(
                "both key and key_str provided",
            ));
        }
        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        let normalized_id = match identity {
            ObjectIdentity::Str(ref sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => {
                return Err(Status::unimplemented(
                    "granular storage currently requires string object IDs",
                ));
            }
        };

        let (entry_key, _) =
            self.resolve_entry_key(req.key, req.key_str.clone())?;
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
        let identity = self.build_identity_numeric_first(
            req.object_id,
            req.object_id_str.clone(),
        )?;
        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        let normalized_id = match identity {
            ObjectIdentity::Str(ref sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => {
                return Err(Status::unimplemented(
                    "granular storage currently requires string object IDs",
                ));
            }
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

        if set_map.is_empty() && !delete_keys.is_empty() {
            if let Some(expected) = expected_version {
                let current_version = shard
                    .get_metadata_granular(&normalized_id)
                    .await?
                    .map(|m| m.object_version)
                    .unwrap_or(0);
                if current_version != expected {
                    return Err(Status::aborted(format!(
                        "Version mismatch: expected {}, got {}",
                        expected, current_version
                    )));
                }
            }
        }

        if !set_map.is_empty() {
            shard
                .batch_set_entries_granular(
                    &normalized_id,
                    set_map,
                    expected_version,
                )
                .await?;
        }

        if !delete_keys.is_empty() {
            shard
                .batch_delete_entries_granular(&normalized_id, delete_keys)
                .await?;
        }

        Ok(Response::new(EmptyResponse {}))
    }

    async fn list_values(
        &self,
        request: tonic::Request<oprc_grpc::ListValuesRequest>,
    ) -> Result<tonic::Response<Self::ListValuesStream>, tonic::Status> {
        self.ensure_granular_enabled()?;
        self.ensure_string_entry_keys_enabled()?;

        let req = request.into_inner();
        let identity = self.build_identity_numeric_first(
            req.object_id,
            req.object_id_str.clone(),
        )?;
        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;

        let normalized_id = match identity {
            ObjectIdentity::Str(ref sid) => sid.clone(),
            ObjectIdentity::Numeric(_) => {
                return Err(Status::unimplemented(
                    "granular storage currently requires string object IDs",
                ));
            }
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

        let metadata = shard.get_metadata_granular(&normalized_id).await?;
        let version = metadata.map(|m| m.object_version).unwrap_or(0);
        let result =
            shard.list_entries_granular(&normalized_id, options).await?;

        let EntryListResult {
            entries,
            next_cursor,
        } = result;
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
