use std::sync::Arc;

use oprc_pb::{
    data_service_server::DataService, EmptyResponse, ObjectResponse,
    SetKeyRequest, SetObjectRequest, ShardStats, SingleKeyRequest,
    SingleObjectRequest, StatsRequest, StatsResponse, ValueResponse,
};
use tonic::{Response, Status};
use tracing::{debug, trace};

use crate::{
    cluster::ObjectDataGridManager,
    shard::{ObjectEntry, ObjectVal, ShardError},
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
        if let Some(entry) = shard.get(&oid).await? {
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
        if let Some(entry) = shard.get(&oid).await? {
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
        shard.delete_with_events(&oid).await?;
        Ok(Response::new(EmptyResponse {}))
    }

    async fn set(
        &self,
        request: tonic::Request<SetObjectRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        trace!("receive set request: {:?}", key_request);
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
        shard.set_with_events(object_id, obj).await?;
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
            let mut obj = ObjectEntry::new();
            obj.value.insert(
                key_request.key,
                ObjectVal::from(key_request.value.unwrap()),
            );
            shard.merge(oid, obj).await?;
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
            let last = shard
                .merge(oid, ObjectEntry::from(key_request.object.unwrap()))
                .await?;
            Ok(Response::new(last.to_resp()))
        } else {
            return Err(Status::invalid_argument("object must not be none"));
        }
    }

    async fn stats(
        &self,
        _request: tonic::Request<StatsRequest>,
    ) -> std::result::Result<tonic::Response<StatsResponse>, tonic::Status>
    {
        let mut iter = self.odgm.shard_manager.shards.first_entry_async().await;
        let mut stats = Vec::new();
        while let Some(entry) = iter {
            let l = entry.shard_state.count().await?;
            let meta = entry.shard_state.meta();
            stats.push(ShardStats {
                shard_id: meta.id,
                collection: meta.collection.clone(),
                partition_id: meta.partition_id as u32,
                count: l,
            });
            iter = entry.next_async().await;
        }

        Ok(Response::new(StatsResponse { shards: stats }))
    }
}

impl From<ShardError> for tonic::Status {
    fn from(value: ShardError) -> Self {
        match value {
            ShardError::MergeError(e) => Status::internal(e.to_string()),
        }
    }
}
