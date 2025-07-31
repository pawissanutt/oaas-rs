use std::sync::Arc;

use oprc_invoke::proxy::ObjectProxy;
use oprc_pb::{
    oprc_function_server::OprcFunction, InvocationRequest, InvocationResponse,
    ObjectInvocationRequest,
};
use tonic::{Request, Response};
use tracing::debug;

use crate::ObjectDataGridManager;

pub struct InvocationService {
    odgm: Arc<ObjectDataGridManager>,
    proxy: ObjectProxy,
}

impl InvocationService {
    pub fn new(
        odgm: Arc<ObjectDataGridManager>,
        z_session: zenoh::Session,
    ) -> Self {
        InvocationService {
            odgm,
            proxy: ObjectProxy::new(z_session),
        }
    }
}

#[tonic::async_trait]
impl OprcFunction for InvocationService {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        let req = request.into_inner();
        debug!("invoke_fn {:?}", req);
        let shard = self.odgm.get_any_local_shard(&req.cls_id).await;
        match shard {
            Some(shard) => {
                debug!(
                    "invoke_fn shard found {:?}",
                    shard.shard_state.meta().id
                );
                shard
                    .inv_offloader
                    .invoke_fn(req)
                    .await
                    .map(Response::new)
                    .map_err(|e| e.into())
            }
            None => {
                let key_expr =
                    format!("oprc/{}/*/invokes/{}", req.cls_id, req.fn_id);
                // let ke = self.z_session.declare_keyexpr(key_expr).await?;
                self.proxy
                    .invoke_fn_raw(&key_expr.try_into().unwrap(), req)
                    .await
                    .map(Response::new)
                    .map_err(|e| e.into())
            }
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
            None => self
                .proxy
                .invoke_obj_fn_raw(req)
                .await
                .map(Response::new)
                .map_err(|e| e.into()),
        }
    }
}
