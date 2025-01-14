use std::sync::Arc;

use oprc_pb::{
    oprc_function_server::OprcFunction, InvocationRequest, InvocationResponse,
    ObjectInvocationRequest,
};
use tonic::{Request, Response, Status};

use crate::error::GatewayError;
use crate::{route::Routable, rpc::RpcManager};
use oprc_offload::conn::ConnManager;

pub struct InvocationHandler {
    conn_manager: Arc<ConnManager<Routable, RpcManager>>,
}

impl InvocationHandler {
    pub fn new(conn_manager: Arc<ConnManager<Routable, RpcManager>>) -> Self {
        Self { conn_manager }
    }
}

#[tonic::async_trait]
impl OprcFunction for InvocationHandler {
    async fn invoke_fn(
        &self,
        request: Request<InvocationRequest>,
    ) -> Result<Response<InvocationResponse>, tonic::Status> {
        let invocation_request = request.into_inner();
        let routable = Routable {
            cls: invocation_request.cls_id.clone(),
            func: invocation_request.fn_id.clone(),
            partition: 0,
        };
        let mut conn = self
            .conn_manager
            .get(routable)
            .await
            .map_err(GatewayError::from)?;
        let resp = conn.invoke_fn(invocation_request).await?;

        Ok(resp)
    }

    async fn invoke_obj(
        &self,
        request: Request<ObjectInvocationRequest>,
    ) -> Result<Response<InvocationResponse>, Status> {
        let invocation_request = request.into_inner();
        let routable = Routable {
            cls: invocation_request.cls_id.clone(),
            func: invocation_request.fn_id.clone(),
            partition: 0,
        };
        let mut conn = self
            .conn_manager
            .get(routable)
            .await
            .map_err(GatewayError::from)?;
        let resp = conn.invoke_obj(invocation_request).await?;
        Ok(resp)
    }
}
