use std::{collections::HashMap, sync::Arc};

use flume::Receiver;
use oprc_offload::OffloadError;
use oprc_pb::{FuncInvokeRoute, InvocationResponse, ResponseStatus};
use oprc_zenoh::util::Handler;
use prost::Message;
use tokio_util::sync::CancellationToken;
use zenoh::query::{Query, Queryable};

use crate::shard::{liveliness::MemberLivelinessState, ShardMetadata};

use super::InvocationOffloader;

pub struct InvocationNetworkManager {
    z_session: zenoh::Session,
    prefix: String,
    meta: ShardMetadata,
    offloader: Arc<InvocationOffloader>,
    queryable_table: HashMap<String, Queryable<Receiver<Query>>>,
}

impl InvocationNetworkManager {
    pub fn new(
        z_session: zenoh::Session,
        meta: ShardMetadata,
        offloader: Arc<InvocationOffloader>,
    ) -> Self {
        let prefix =
            format!("oprc/{}/{}", meta.collection.clone(), meta.partition_id,);
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            prefix,
            meta,
            queryable_table: HashMap::new(),
            offloader,
        }
    }

    pub async fn start(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let routes = self.meta.invocations.fn_routes.clone();
        let need_primiry = self
            .meta
            .options
            .get("invoke_only_primary")
            .map(|s| s == "true")
            .unwrap_or(false);
        let should_set_invoke =
            !need_primiry || self.meta.primary == Some(self.meta.id);
        if should_set_invoke {
            for (fn_id, route) in routes.iter() {
                if route.standby {
                    continue;
                }
                self.start_invoke_loop(route, fn_id).await?;
            }
        } else {
            tracing::info!(
                "shard {}: skip starting invoke loop, not primary",
                self.meta.id
            );
        }
        Ok(())
    }

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
        let handler = InvokeHandler::new(&self.meta, self.offloader.clone());
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

    pub async fn on_liveliness_updated(
        &mut self,
        state: &MemberLivelinessState,
    ) {
        let routes = self.meta.invocations.fn_routes.clone();
        for (fn_id, route) in routes.iter() {
            if !route.standby {
                continue;
            }
            let mut should_active = true;
            let active_group = if route.active_group.is_empty() {
                &self.meta.replica
            } else {
                &route.active_group
            };
            for active_id in active_group.iter() {
                if active_id == &self.meta.id {
                    continue;
                }
                let live = state
                    .liveliness_map
                    .get(active_id)
                    .map(|e| e.to_owned())
                    .unwrap_or(false);
                should_active &= !live;
            }
            tracing::info!(
                "shard {}: invocation {} should be active: {should_active}, active group: {active_group:?}, liveliness: {:?}",
                self.meta.id, fn_id, state.liveliness_map
            );
            if should_active {
                if let Err(err) = self.start_invoke_loop(route, fn_id).await {
                    tracing::error!(
                        "shard {}: failed to start invoke loop for {}: {:?}",
                        self.meta.id,
                        fn_id,
                        err
                    );
                };
            } else {
                let q = self.queryable_table.remove(fn_id);
                if let Some(q) = q {
                    tracing::info!(
                        "shard {}: undeclare invocation loop for {}",
                        self.meta.id,
                        fn_id
                    );
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
    metadata: ShardMetadata,
    offloader: Arc<InvocationOffloader>,
}

impl InvokeHandler {
    pub fn new(
        meta: &ShardMetadata,
        offlorder: Arc<InvocationOffloader>,
    ) -> Self {
        Self {
            metadata: meta.clone(),
            offloader: offlorder,
        }
    }

    async fn handle_invoke_fn(&self, query: Query) {
        match decode(&query) {
            Ok(req) => {
                match self.offloader.invoke_fn(req).await {
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
                match self.offloader.invoke_obj(req).await {
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
        ..Default::default()
    };
    if let Err(e) = query.reply(query.key_expr(), resp.encode_to_vec()).await {
        tracing::warn!("Failed to reply error: {:?}", e);
    }
}
