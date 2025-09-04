use oprc_pb::{
    EmptyResponse, ObjectResponse, SetKeyRequest, SetObjectRequest, ShardStats,
    SingleKeyRequest, SingleObjectRequest, StatsRequest, StatsResponse,
    ValueResponse, data_service_server::DataService,
};
use std::sync::Arc;
use tonic::{Response, Status};
use tracing::{debug, trace};

use crate::{
    cluster::ObjectDataGridManager,
    shard::{
        ObjectEntry, ObjectVal, basic::ObjectError, unified::config::ShardError,
    },
};

pub struct OdgmDataService {
    odgm: Arc<ObjectDataGridManager>,
}

impl OdgmDataService {
    pub fn new(odgm: Arc<ObjectDataGridManager>) -> Self {
        OdgmDataService { odgm }
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
        let oid = key_request.object_id;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        if let Some(entry) = shard.get_object(oid).await? {
            Ok(Response::new(entry.to_resp()))
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
        let oid = key_request.object_id;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        if let Some(entry) = shard.get_object(oid).await? {
            let val = entry.value.get(&key_request.key);
            if let Some(v) = val {
                return Ok(Response::new(ValueResponse {
                    value: Some(v.into_val()),
                }));
            }
            return Ok(Response::new(ValueResponse { value: None }));
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
        let oid = key_request.object_id;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        shard.delete_object(&oid).await?;
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
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        let object_id = key_request.object_id;
        let obj = ObjectEntry::from(key_request.object.unwrap());
        shard.set_object(object_id, obj).await?;
        Ok(Response::new(EmptyResponse {}))
    }

    async fn set_value(
        &self,
        request: tonic::Request<SetKeyRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        let oid = key_request.object_id;
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
            // Get existing object or create new one
            let mut obj = if let Some(existing) = shard.get_object(oid).await? {
                existing
            } else {
                ObjectEntry::new()
            };

            // Update the specific key
            obj.value.insert(
                key_request.key,
                ObjectVal::from(key_request.value.unwrap()),
            );

            // Set the updated object
            shard.set_object(oid, obj).await?;
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
        let oid = key_request.object_id;
        let shard = self
            .odgm
            .get_local_shard(
                &key_request.cls_id,
                key_request.partition_id as u16,
            )
            .await
            .ok_or_else(|| Status::not_found("not found shard"))?;
        if key_request.object.is_some() {
            let new_obj = ObjectEntry::from(key_request.object.unwrap());

            // Get existing object or use the new one
            let merged_obj =
                if let Some(mut existing) = shard.get_object(oid).await? {
                    // Merge the new object into the existing one
                    existing.merge(new_obj)?;
                    existing
                } else {
                    new_obj
                };

            // Set the merged object
            shard.set_object(oid, merged_obj.clone()).await?;
            Ok(Response::new(merged_obj.to_resp()))
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
