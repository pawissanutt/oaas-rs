use oprc_grpc::{
    EmptyResponse, ObjectResponse, SetKeyRequest, SetObjectRequest, ShardStats,
    SingleKeyRequest, SingleObjectRequest, StatsRequest, StatsResponse,
    ValueResponse, data_service_server::DataService,
};
use std::sync::Arc;
use tonic::{Response, Status};
use tracing::{debug, trace};

use crate::identity::{ObjectIdentity, build_identity};
use crate::metrics::{
    incr_entry_mutation, incr_get, incr_set, record_normalize_latency_ms,
};
use crate::{
    cluster::ObjectDataGridManager,
    shard::{
        ObjectEntry, ObjectVal, basic::ObjectError, unified::config::ShardError,
    },
};

pub struct OdgmDataService {
    odgm: Arc<ObjectDataGridManager>,
    max_string_id_len: usize,
    enable_string_ids: bool,
    enable_string_entry_keys: bool,
}

impl OdgmDataService {
    pub fn new(
        odgm: Arc<ObjectDataGridManager>,
        max_string_id_len: usize,
        enable_string_ids: bool,
        enable_string_entry_keys: bool,
    ) -> Self {
        OdgmDataService {
            odgm,
            max_string_id_len,
            enable_string_ids,
            enable_string_entry_keys,
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
}

#[tonic::async_trait]
impl DataService for OdgmDataService {
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
        let entry_opt = match identity {
            ObjectIdentity::Numeric(oid) => shard.get_object(oid).await?,
            ObjectIdentity::Str(ref sid) => {
                shard.get_object_by_str_id(sid).await?
            }
        };
        let variant = match &identity {
            ObjectIdentity::Numeric(_) => "numeric",
            ObjectIdentity::Str(_) => "string",
        };
        incr_get(variant);
        entry_opt
            .map(|e| {
                match identity {
                    ObjectIdentity::Numeric(_) => Response::new(e.to_resp()),
                    ObjectIdentity::Str(ref sid) => {
                        // Inject metadata with object_id_str
                        let mut data = e.to_data();
                        data.metadata = Some(oprc_grpc::ObjMeta {
                            cls_id: key_request.cls_id.clone(),
                            partition_id: key_request.partition_id as u32,
                            object_id: 0,
                            object_id_str: Some(sid.clone()),
                        });
                        Response::new(oprc_grpc::ObjectResponse {
                            obj: Some(data),
                        })
                    }
                }
            })
            .ok_or_else(|| Status::not_found("not found data"))
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
        let entry_opt = match identity {
            ObjectIdentity::Numeric(oid) => shard.get_object(oid).await?,
            ObjectIdentity::Str(ref sid) => {
                shard.get_object_by_str_id(sid).await?
            }
        };
        if let Some(entry) = entry_opt {
            let variant = match &identity {
                ObjectIdentity::Numeric(_) => "numeric",
                ObjectIdentity::Str(_) => "string",
            };
            incr_get(variant);
            if let Some(ref kstr) = key_request.key_str {
                if let Some(v) = entry.str_value.get(kstr) {
                    return Ok(Response::new(ValueResponse {
                        value: Some(v.into_val()),
                    }));
                }
                return Ok(Response::new(ValueResponse { value: None }));
            } else {
                let val = entry.value.get(&key_request.key);
                if let Some(v) = val {
                    return Ok(Response::new(ValueResponse {
                        value: Some(v.into_val()),
                    }));
                }
                return Ok(Response::new(ValueResponse { value: None }));
            }
        }
        Err(Status::not_found("not found data"))
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
        let obj = ObjectEntry::from(obj_raw);
        // Duplicate create semantics: fail if exists (AlreadyExists); Merge endpoint handles updates.
        match identity {
            ObjectIdentity::Numeric(oid) => {
                // Preserve legacy upsert behavior for numeric IDs (needed for event update tests).
                let existed = shard.get_object(oid).await?.is_some();
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
        // let object_id = key_request.object_id;
        if key_request.value.is_some() {
            let mut obj = match identity {
                ObjectIdentity::Numeric(oid) => shard.get_object(oid).await?,
                ObjectIdentity::Str(ref sid) => {
                    shard.get_object_by_str_id(sid).await?
                }
            }
            .unwrap_or_else(|| ObjectEntry::new());
            let oval = ObjectVal::from(key_request.value.unwrap());
            if let Some(ref kstr) = key_request.key_str {
                obj.str_value.insert(kstr.clone(), oval);
                incr_entry_mutation("string");
            } else {
                obj.value.insert(key_request.key, oval);
                incr_entry_mutation("numeric");
            }
            match identity {
                ObjectIdentity::Numeric(oid) => {
                    shard.set_object(oid, obj).await?
                }
                ObjectIdentity::Str(ref sid) => {
                    shard.set_object_by_str_id(sid, obj).await?
                }
            }
            Ok(Response::new(EmptyResponse {}))
        } else {
            return Err(Status::invalid_argument("object must not be none"));
        }
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
            let new_obj = ObjectEntry::from(raw);
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
            granular_entry_storage: false,
        };
        Ok(Response::new(resp))
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
        }
    }
}
