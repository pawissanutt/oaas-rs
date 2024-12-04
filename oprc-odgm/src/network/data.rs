use std::sync::Arc;

use flare_dht::{shard::KvShard, FlareNode};
use oprc_pb::{
    data_service_server::DataService, EmptyResponse, ObjectReponse,
    SetKeyRequest, SetObjectRequest, SingleKeyRequest, SingleObjectRequest,
    ValueResponse,
};
use tonic::{Response, Status};
use tracing::error;

use crate::shard::{ObjectEntry, ObjectShard, ObjectVal, ShardError};

pub struct OdgmDataService {
    flare: Arc<FlareNode<ObjectShard>>,
}

impl OdgmDataService {
    pub fn new(flare: Arc<FlareNode<ObjectShard>>) -> Self {
        OdgmDataService { flare }
    }
}

#[tonic::async_trait]
impl DataService for OdgmDataService {
    async fn get(
        &self,
        request: tonic::Request<SingleObjectRequest>,
    ) -> std::result::Result<tonic::Response<ObjectReponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        let oid = key_request.object_id;
        let shard = self
            .flare
            .get_shard(&key_request.cls_id, &oid.to_be_bytes())
            .await?;
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
        let oid = key_request.object_id;
        let shard = self
            .flare
            .get_shard(&key_request.cls_id, &oid.to_be_bytes())
            .await?;
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
        let oid = key_request.object_id;
        let shard = self
            .flare
            .get_shard(&key_request.cls_id, &oid.to_be_bytes())
            .await?;
        shard.delete(&oid).await?;
        Ok(Response::new(EmptyResponse {}))
    }

    async fn set(
        &self,
        request: tonic::Request<SetObjectRequest>,
    ) -> std::result::Result<tonic::Response<EmptyResponse>, tonic::Status>
    {
        let key_request = request.into_inner();
        let oid = key_request.object_id;
        let shard = self
            .flare
            .get_shard(&key_request.cls_id, &oid.to_be_bytes())
            .await?;
        let object_id = key_request.object_id;
        let obj = ObjectEntry::from_data(key_request.object.unwrap());
        shard.set(object_id, obj).await?;
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
            .flare
            .get_shard(&key_request.cls_id, &oid.to_be_bytes())
            .await?;
        let object_id = key_request.object_id;
        if key_request.value.is_some() {
            shard
                .modify(&object_id, |obj| {
                    obj.value.insert(
                        key_request.key.clone(),
                        ObjectVal::from_val_owned(key_request.value.unwrap()),
                    );
                })
                .await?;
        }

        Ok(Response::new(EmptyResponse {}))
    }

    async fn merge(
        &self,
        request: tonic::Request<SetObjectRequest>,
    ) -> Result<tonic::Response<ObjectReponse>, tonic::Status> {
        let key_request = request.into_inner();
        let oid = key_request.object_id;
        let shard = self
            .flare
            .get_shard(&key_request.cls_id, &oid.to_be_bytes())
            .await?;
        let object_id = key_request.object_id;
        if key_request.object.is_some() {
            let out = shard
                .modify(&object_id, |obj| {
                    let r = obj.merge(&ObjectEntry::from_data(
                        key_request.object.unwrap(),
                    ));
                    if let Err(e) = r {
                        error!("merge error {}", e);
                    }
                    obj.clone()
                })
                .await?;
            Ok(Response::new(out.to_resp()))
        } else {
            return Err(Status::invalid_argument("object must not be none"));
        }
    }
}

impl From<ShardError> for tonic::Status {
    fn from(value: ShardError) -> Self {
        match value {
            ShardError::MergeError(e) => Status::internal(e.to_string()),
        }
    }
}
