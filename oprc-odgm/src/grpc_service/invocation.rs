use std::sync::Arc;

use oprc_pb::{
    oprc_function_server::OprcFunction, InvocationRequest, InvocationResponse,
    ObjectInvocationRequest,
};
use tonic::{Request, Response, Status};

use crate::ObjectDataGridManager;

pub struct InvocationService {
    odgm: Arc<ObjectDataGridManager>,
}

impl InvocationService {
    pub fn new(odgm: Arc<ObjectDataGridManager>) -> Self {
        InvocationService { odgm }
    }
}

#[tonic::async_trait]
impl OprcFunction for InvocationService {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        let req = request.into_inner();
        let shard = self.odgm.get_any_local_shard(&req.cls_id).await;
        match shard {
            Some(shard) => shard
                .inv_offloader
                .invoke_fn(req)
                .await
                .map(Response::new)
                .map_err(|e| e.into()),
            None => Err(Status::not_found("shard not found")),
        }
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        let req = request.into_inner();
        let shard = self
            .odgm
            .get_local_shard(&req.cls_id, req.partition_id as u16)
            .await;
        match shard {
            Some(shard) => shard
                .inv_offloader
                .invoke_obj(req)
                .await
                .map(Response::new)
                .map_err(|e| e.into()),
            None => Err(Status::not_found("shard not found")),
        }
    }
}
