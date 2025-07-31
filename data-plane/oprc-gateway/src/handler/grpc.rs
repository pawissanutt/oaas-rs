use oprc_pb::{
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
    oprc_function_server::OprcFunction,
};
use tonic::{Request, Response, Status};

use crate::error::GatewayError;
use oprc_invoke::{Invoker, route::Routable};

pub struct InvocationHandler {
    conn_manager: Invoker,
}

impl InvocationHandler {
    pub fn new(conn_manager: Invoker) -> Self {
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
            .get_conn(routable)
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
            .get_conn(routable)
            .await
            .map_err(GatewayError::from)?;
        let resp = conn.invoke_obj(invocation_request).await?;
        Ok(resp)
    }
}
