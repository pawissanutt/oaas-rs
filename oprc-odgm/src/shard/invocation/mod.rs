use std::sync::Arc;

use oprc_offload::{
    conn::{ConnFactory, ConnManager},
    grpc::RpcManager,
    OffloadError,
};
use oprc_pb::{
    InvocationRequest, InvocationResponse, InvocationRoute,
    ObjectInvocationRequest, ResponseStatus,
};
use oprc_zenoh::util::Handler;
use prost::Message;
use tokio_util::sync::CancellationToken;
use zenoh::query::Query;

use super::ShardMetadata;

pub struct InvocationOffloader {
    z_session: zenoh::Session,
    token: CancellationToken,
    prefix: String,
    meta: ShardMetadata,
}

impl InvocationOffloader {
    pub fn new(z_session: zenoh::Session, meta: ShardMetadata) -> Self {
        let prefix =
            format!("oprc/{}/{}", meta.collection.clone(), meta.partition_id,);
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            token: CancellationToken::new(),
            prefix,
            meta,
        }
    }

    pub async fn start(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.token = CancellationToken::new();
        for (fn_id, route) in self.meta.invocations.fn_routes.iter() {
            let key = match route.stateless {
                true => format!("{}/invokes/{}", self.prefix, fn_id),
                false => format!("{}/objects/*/invokes/{}", self.prefix, fn_id),
            };
            self.start_invoke_loop(key).await?;
        }
        Ok(())
    }

    // pub fn is_running(&self) -> bool {
    //     !self.token.is_cancelled()
    // }

    pub async fn start_invoke_loop(
        &self,
        key: String,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let handler = InvokeHandler::new(&self.meta);
        oprc_zenoh::util::declare_queryable_loop(
            &self.z_session,
            self.token.clone(),
            key,
            handler,
            16,
            1024,
        )
        .await?;
        Ok(())
    }

    pub fn stop(&self) {
        self.token.cancel();
    }
}

#[derive(Clone)]
struct InvokeHandler {
    conn_manager: Arc<ConnManager<String, RpcManager>>,
    metadata: ShardMetadata,
}

impl InvokeHandler {
    pub fn new(meta: &ShardMetadata) -> Self {
        let factory = Arc::new(FnConnFactory::new(&meta));
        let conf = oprc_offload::conn::PoolConfig::default();
        let conn =
            Arc::new(oprc_offload::conn::ConnManager::new(factory, conf));
        Self {
            conn_manager: conn,
            metadata: meta.clone(),
        }
    }

    async fn invoke_fn(
        &self,
        req: InvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        let mut conn = self.conn_manager.get(req.fn_id.clone()).await?;
        let resp = conn
            .invoke_fn(req)
            .await
            .map_err(|e| OffloadError::GrpcError(e))?;
        Ok(resp.into_inner())
    }

    async fn invoke_obj(
        &self,
        req: ObjectInvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        let mut conn = self.conn_manager.get(req.fn_id.clone()).await?;
        let resp = conn
            .invoke_obj(req)
            .await
            .map_err(|e| OffloadError::GrpcError(e))?;
        Ok(resp.into_inner())
    }

    async fn handle_invoke_fn(&self, query: Query) {
        match decode(&query) {
            Ok(req) => {
                match self.invoke_fn(req).await {
                    Ok(resp) => {
                        write_message(&query, resp).await;
                    }
                    Err(OffloadError::GrpcError(s)) => {
                        write_error(&query, s, ResponseStatus::AppError as i32)
                            .await;
                    }
                    Err(e) => {
                        write_error(
                            &query,
                            e,
                            ResponseStatus::SystemError as i32,
                        )
                        .await;
                    }
                };
            }
            Err(e) => {
                write_error(&query, e, ResponseStatus::InvalidRequest as i32)
                    .await;
            }
        }
    }

    async fn handle_invoke_obj(&self, query: Query) {
        match decode(&query) {
            Ok(req) => {
                match self.invoke_obj(req).await {
                    Ok(resp) => {
                        write_message(&query, resp).await;
                    }
                    Err(OffloadError::GrpcError(s)) => {
                        write_error(&query, s, ResponseStatus::AppError as i32)
                            .await;
                    }
                    Err(e) => {
                        write_error(
                            &query,
                            e,
                            ResponseStatus::SystemError as i32,
                        )
                        .await;
                    }
                };
            }
            Err(e) => {
                write_error(&query, e, ResponseStatus::InvalidRequest as i32)
                    .await;
                return;
            }
        }
    }
}

#[async_trait::async_trait]
impl Handler<Query> for InvokeHandler {
    async fn handle(&self, query: Query) {
        let is_object = match query.key_expr().split("/").skip(3).next() {
            Some(path) => path == "objects",
            None => {
                return;
            }
        };
        tracing::debug!(
            "shard {}: received invocation, '{}'",
            self.metadata.id,
            query.key_expr()
        );
        if is_object {
            self.handle_invoke_obj(query).await;
        } else {
            self.handle_invoke_fn(query).await;
        }
    }
}

fn decode<M>(query: &Query) -> Result<M, String>
where
    M: Message + Default,
{
    match query.payload() {
        Some(payload) => match M::decode(payload.to_bytes().as_ref()) {
            Ok(msg) => Ok(msg),
            Err(e) => Err(e.to_string()),
        },
        None => Err("Payload must not be empty".into()),
    }
}

async fn write_message<M: Message>(query: &Query, msg: M) {
    let byte = msg.encode_to_vec();
    if let Err(e) = query.reply(query.key_expr(), byte).await {
        write_error(query, e, ResponseStatus::SystemError as i32).await;
    }
}

async fn write_error<E: ToString>(query: &Query, e: E, status: i32) {
    let resp = InvocationResponse {
        payload: Some(e.to_string().into_bytes()),
        status,
    };
    if let Err(e) = query.reply_err(resp.encode_to_vec()).await {
        tracing::warn!("Failed to reply error: {:?}", e);
    }
}

struct FnConnFactory {
    table: InvocationRoute,
    cls_id: String,
}

impl FnConnFactory {
    pub fn new(meta: &ShardMetadata) -> Self {
        Self {
            table: meta.invocations.clone(),
            cls_id: meta.collection.clone(),
        }
    }
}

#[async_trait::async_trait]
impl ConnFactory<String, RpcManager> for FnConnFactory {
    async fn create(&self, key: String) -> Result<RpcManager, OffloadError> {
        if let Some(fn_route) = self.table.fn_routes.get(&key) {
            tracing::debug!(
                "create connection for fn={}, url={}",
                key,
                fn_route.url
            );
            Ok(RpcManager::new(&fn_route.url)?)
        } else {
            Err(OffloadError::NoFunc(self.cls_id.clone(), key))
        }
    }
}
