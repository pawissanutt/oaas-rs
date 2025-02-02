use std::{collections::HashMap, sync::Arc};

use flume::Receiver;
use oprc_offload::{
    conn::{ConnFactory, ConnManager},
    grpc::RpcManager,
    OffloadError,
};
use oprc_pb::{
    FuncInvokeRoute, InvocationRequest, InvocationResponse, InvocationRoute,
    ObjectInvocationRequest, ResponseStatus,
};
use oprc_zenoh::util::Handler;
use prost::Message;
use tokio_util::sync::CancellationToken;
use zenoh::query::{Query, Queryable};

use super::{liveliness::MemberLivelinessState, ShardMetadata};

pub struct InvocationOffloader {
    z_session: zenoh::Session,
    prefix: String,
    meta: ShardMetadata,
    queryable_table: HashMap<String, Queryable<Receiver<Query>>>,
}

impl InvocationOffloader {
    pub fn new(z_session: zenoh::Session, meta: ShardMetadata) -> Self {
        let prefix =
            format!("oprc/{}/{}", meta.collection.clone(), meta.partition_id,);
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            prefix,
            meta,
            queryable_table: HashMap::new(),
        }
    }

    pub async fn start(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let routes = self.meta.invocations.fn_routes.clone();
        for (fn_id, route) in routes.iter() {
            self.start_invoke_loop(route, fn_id).await?;
        }
        Ok(())
    }

    // pub fn is_running(&self) -> bool {
    //     !self.token.is_cancelled()
    // }

    pub async fn start_invoke_loop(
        &mut self,
        route: &FuncInvokeRoute,
        fn_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.queryable_table.contains_key(fn_id) {
            return Ok(());
        }
        let key = match route.stateless {
            true => format!("{}/invokes/{}", self.prefix, fn_id),
            false => format!("{}/objects/*/invokes/{}", self.prefix, fn_id),
        };
        let handler = InvokeHandler::new(&self.meta);
        tracing::info!("shard {}: declare queryable {}", self.meta.id, key);
        let q = oprc_zenoh::util::declare_managed_queryable(
            &self.z_session,
            key,
            handler,
            64,
            65536,
        )
        .await?;
        self.queryable_table.insert(fn_id.to_string(), q);
        Ok(())
    }

    pub async fn on_lost_liveliness(&mut self, state: &MemberLivelinessState) {
        let routes = self.meta.invocations.fn_routes.clone();
        for (fn_id, route) in routes.iter() {
            if route.standby {
                let mut should_active = true;
                for active_id in route.active_group.iter() {
                    let live = state
                        .liveliness_map
                        .get(active_id)
                        .map(|e| e.to_owned())
                        .unwrap_or(false);
                    should_active &= !live;
                }
                if should_active {
                    if let Err(err) = self.start_invoke_loop(route, fn_id).await
                    {
                        tracing::error!(
                            "shard {}: failed to start invoke loop for {}: {:?}",
                            self.meta.id,
                            fn_id,
                            err
                        );
                    };
                } else {
                    tracing::info!(
                        "shard {}: undeclare invocation loop for {}",
                        self.meta.id,
                        fn_id
                    );
                    let q = self.queryable_table.remove(fn_id);
                    if let Some(q) = q {
                        if let Err(e) = q.undeclare().await {
                            tracing::error!(
                                "shard {}: failed to undeclare queryable {}: {:?}",
                                self.meta.id,
                                fn_id,
                                e
                            );
                        };
                    }
                }
            }
        }
    }

    pub async fn stop(&mut self) {
        let all_fn: Vec<String> =
            self.queryable_table.keys().cloned().collect();

        for fn_id in all_fn.iter() {
            if let Some(q) = self.queryable_table.remove(fn_id) {
                if let Err(e) = q.undeclare().await {
                    tracing::warn!("Failed to undeclare queryable: {:?}", e);
                };
            }
        }
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
        let pool_size: u64 = meta
            .options
            .get("offload_max_pool_size")
            .unwrap_or(&"64".to_string())
            .parse()
            .unwrap_or(64);
        let conf = oprc_offload::conn::PoolConfig {
            max_open: pool_size,
            ..Default::default()
        };
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
    if let Err(e) = query.reply(query.key_expr(), resp.encode_to_vec()).await {
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
            Ok(RpcManager::new(&fn_route.url)?)
        } else {
            Err(OffloadError::NoFunc(self.cls_id.clone(), key))
        }
    }
}
