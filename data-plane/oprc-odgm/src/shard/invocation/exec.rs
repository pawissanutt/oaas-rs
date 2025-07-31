use std::{sync::Arc, time::Duration};

use crate::shard::ShardMetadata;
use oprc_invoke::{
    conn::{ConnFactory, ConnManager},
    grpc::RpcManager,
    handler::InvocationExecutor,
    OffloadError,
};
use oprc_pb::{
    InvocationRequest, InvocationResponse, InvocationRoute,
    ObjectInvocationRequest,
};

#[derive(Clone)]
pub struct InvocationOffloader {
    conn_manager: Arc<ConnManager<String, RpcManager>>,
}

#[async_trait::async_trait]
impl InvocationExecutor for InvocationOffloader {
    async fn invoke_fn(
        &self,
        req: oprc_pb::InvocationRequest,
    ) -> Result<oprc_pb::InvocationResponse, oprc_invoke::OffloadError> {
        self.invoke_fn(req).await
    }

    async fn invoke_obj(
        &self,
        req: oprc_pb::ObjectInvocationRequest,
    ) -> Result<oprc_pb::InvocationResponse, oprc_invoke::OffloadError> {
        self.invoke_obj(req).await
    }
}

impl InvocationOffloader {
    pub fn new(meta: &ShardMetadata) -> Self {
        let factory = Arc::new(FnConnFactory::new(
            meta.invocations.clone(),
            meta.collection.clone(),
        ));
        let pool_size: u64 = meta
            .options
            .get("offload_max_pool_size")
            .unwrap_or(&"64".to_string())
            .parse()
            .unwrap_or(64);
        let pool_max_idle_lifetime = meta
            .options
            .get("pool_max_idle_lifetime")
            .unwrap_or(&"30000".to_string())
            .parse()
            .unwrap_or(30000);
        let pool_max_lifetime = meta
            .options
            .get("pool_max_lifetime")
            .unwrap_or(&"600000".to_string())
            .parse()
            .unwrap_or(600000);
        let conf = oprc_invoke::conn::PoolConfig {
            max_open: pool_size,
            max_idle_lifetime: Some(Duration::from_millis(
                pool_max_idle_lifetime,
            )),
            max_lifetime: Some(Duration::from_millis(pool_max_lifetime)),
            ..Default::default()
        };
        let conn = Arc::new(oprc_invoke::conn::ConnManager::new(factory, conf));
        Self {
            conn_manager: conn,
            // metadata: meta.clone(),
        }
    }

    pub async fn invoke_fn(
        &self,
        req: InvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        let mut conn = self.conn_manager.get(req.fn_id.clone()).await?;
        let mut req = tonic::Request::new(req);
        req.set_timeout(Duration::from_secs(300));
        let resp = conn
            .invoke_fn(req)
            .await
            .map_err(|e| OffloadError::GrpcError(e))?;
        Ok(resp.into_inner())
    }

    pub async fn invoke_obj(
        &self,
        req: ObjectInvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        let mut conn = self.conn_manager.get(req.fn_id.clone()).await?;
        let mut req = tonic::Request::new(req);
        req.set_timeout(Duration::from_secs(300));
        let resp = conn
            .invoke_obj(req)
            .await
            .map_err(|e| OffloadError::GrpcError(e))?;
        Ok(resp.into_inner())
    }
}

struct FnConnFactory {
    table: InvocationRoute,
    cls_id: String,
}

impl FnConnFactory {
    pub fn new(route: InvocationRoute, cls_id: String) -> Self {
        Self {
            table: route,
            cls_id,
        }
    }
}

#[async_trait::async_trait]
impl ConnFactory<String, RpcManager> for FnConnFactory {
    async fn create(&self, key: String) -> Result<RpcManager, OffloadError> {
        if let Some(fn_route) = self.table.fn_routes.get(&key) {
            Ok(RpcManager::new(&fn_route.url)?)
        } else {
            Err(OffloadError::NoFunc(self.cls_id.clone(), key))
        }
    }
}
