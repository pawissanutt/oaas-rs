use std::{sync::Arc, time::Duration};

use crate::events::{
    EventManager,
    types::{EventContext, EventType},
};
use crate::shard::ShardMetadata;
use oprc_invoke::{
    OffloadError,
    conn::{ConnFactory, ConnManager},
    grpc::RpcManager,
    handler::InvocationExecutor,
};
use oprc_pb::{
    InvocationRequest, InvocationResponse, InvocationRoute,
    ObjectInvocationRequest,
};
use tracing::{debug, warn};

#[derive(Clone)]
pub struct InvocationOffloader<E: EventManager + Send + Sync + 'static> {
    conn_manager: Arc<ConnManager<String, RpcManager>>,
    event_manager: Option<Arc<E>>,
    metadata: ShardMetadata,
}

#[async_trait::async_trait]
impl<E: EventManager + Send + Sync + 'static> InvocationExecutor
    for InvocationOffloader<E>
{
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

impl<E: EventManager + Send + Sync + 'static> InvocationOffloader<E> {
    pub fn new(meta: &ShardMetadata, event_manager: Option<Arc<E>>) -> Self {
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
            event_manager,
            metadata: meta.clone(),
        }
    }

    pub fn with_event_manager(mut self, event_manager: Arc<E>) -> Self {
        self.event_manager = Some(event_manager);
        self
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
        let object_id = req.object_id;
        let fn_id = req.fn_id.clone();
        let partition_id = req.partition_id;

        debug!(
            "Invoking function {} on object {} in partition {}",
            fn_id, object_id, partition_id
        );

        let mut conn = self.conn_manager.get(req.fn_id.clone()).await?;
        let mut req = tonic::Request::new(req);
        req.set_timeout(Duration::from_secs(300));

        let result = conn
            .invoke_obj(req)
            .await
            .map_err(|e| OffloadError::GrpcError(e));

        // Trigger appropriate events if event manager is available
        if let Some(event_manager) = &self.event_manager {
            let event_context = EventContext {
                object_id,
                class_id: self.metadata.collection.clone(),
                partition_id: partition_id as u16,
                event_type: match &result {
                    Ok(_response) => {
                        debug!(
                            "Function {} completed successfully for object {}",
                            fn_id, object_id
                        );
                        EventType::FunctionComplete(fn_id.clone())
                    }
                    Err(e) => {
                        warn!(
                            "Function {} failed for object {}: {}",
                            fn_id, object_id, e
                        );
                        EventType::FunctionError(fn_id.clone())
                    }
                },
                payload: match &result {
                    Ok(response) => response.get_ref().payload.clone(),
                    Err(_) => None,
                },
                error_message: match &result {
                    Ok(_) => None,
                    Err(e) => Some(format!("{}", e)),
                },
            };

            // Trigger the event asynchronously (fire and forget)
            event_manager.trigger_event(event_context).await;
        }

        result.map(|resp| resp.into_inner())
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
