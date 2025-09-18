use std::{collections::HashMap, sync::Arc};

use crate::events::EventManager;
use flume::Receiver;
use oprc_grpc::FuncInvokeRoute;
use oprc_invoke::handler::{AsyncInvocationHandler, InvocationZenohHandler};
use oprc_zenoh::util::{
    ManagedConfig, declare_managed_queryable, declare_managed_subscriber,
};
use tokio_util::sync::CancellationToken;
use zenoh::{
    pubsub::Subscriber,
    query::{Query, Queryable},
    sample::Sample,
};

use crate::shard::{ShardMetadata, liveliness::MemberLivelinessState};

use super::InvocationOffloader;

/// Network manager for handling both synchronous and asynchronous invocations over Zenoh.
///
/// This manager orchestrates the lifecycle of Zenoh primitives for different invocation patterns:
/// - Synchronous invocations: Uses Queryable for GET requests that expect immediate responses
/// - Asynchronous invocations: Uses Subscriber for PUT requests that process invocations without immediate responses
///
/// Key patterns:
/// - GET `oprc/<class>/<partition>/invokes/<method_id>` -> synchronous stateless function invocation
/// - GET `oprc/<class>/<partition>/objects/<object_id>/invokes/<method_id>` -> synchronous object method invocation  
/// - PUT `oprc/<class>/<partition>/invokes/<method_id>/async/<invocation_id>` -> asynchronous stateless function invocation
/// - PUT `oprc/<class>/<partition>/objects/<object_id>/invokes/<method_id>/async/<invocation_id>` -> asynchronous object method invocation
pub struct InvocationNetworkManager<E: EventManager + Send + Sync + 'static> {
    z_session: zenoh::Session,
    prefix: String,
    meta: ShardMetadata,
    offloader: Arc<InvocationOffloader<E>>,
    /// Table of active queryables for synchronous invocations (GET requests)
    queryable_table: HashMap<String, Queryable<Receiver<Query>>>,
    /// Table of active subscribers for asynchronous invocations (PUT requests)
    async_subscriber_table: HashMap<String, Subscriber<Receiver<Sample>>>,
}

impl<E: EventManager + Send + Sync + 'static> InvocationNetworkManager<E> {
    /// Create a new invocation network manager
    pub fn new(
        z_session: zenoh::Session,
        meta: ShardMetadata,
        offloader: Arc<InvocationOffloader<E>>,
    ) -> Self {
        let prefix =
            format!("oprc/{}/{}", meta.collection.clone(), meta.partition_id);
        let token = CancellationToken::new();
        token.cancel();
        Self {
            z_session,
            prefix,
            meta,
            queryable_table: HashMap::new(),
            async_subscriber_table: HashMap::new(),
            offloader,
        }
    }

    /// Start all invocation loops (both sync and async) for the configured function routes
    pub async fn start(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let routes = self.meta.invocations.fn_routes.clone();
        let need_primary = self
            .meta
            .options
            .get("invoke_only_primary")
            .map(|s| s == "true")
            .unwrap_or(false);
        let should_set_invoke =
            !need_primary || self.meta.primary == Some(self.meta.id);

        if should_set_invoke {
            for (fn_id, route) in routes.iter() {
                if route.standby {
                    continue;
                }
                // Start both sync and async invocation endpoints
                self.start_invoke_loop(route, fn_id).await?;
                self.start_async_invoke_loop(route, fn_id).await?;
            }
        } else {
            tracing::info!(
                "shard {}: skip starting invoke loop, not primary",
                self.meta.id
            );
        }
        Ok(())
    }

    /// Start a synchronous invocation loop using Queryable for GET requests
    /// This handles immediate request-response patterns
    pub async fn start_invoke_loop(
        &mut self,
        route: &FuncInvokeRoute,
        fn_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.queryable_table.contains_key(fn_id) {
            return Ok(());
        }

        // Build the key expression based on whether the function is stateless or stateful
        let key = match route.stateless {
            true => format!("{}/invokes/{}", self.prefix, fn_id),
            false => format!("{}/objects/*/invokes/{}", self.prefix, fn_id),
        };

        let handler = InvocationZenohHandler::new(
            format!("Shard {}", self.meta.id),
            self.offloader.clone(),
        );

        tracing::info!("shard {}: declare queryable {}", self.meta.id, key);
        let config = ManagedConfig::new(key, 64, 65536);
        let queryable =
            declare_managed_queryable(&self.z_session, config, handler).await?;
        self.queryable_table.insert(fn_id.to_string(), queryable);
        Ok(())
    }

    /// Start an asynchronous invocation loop using Subscriber for PUT requests
    /// This handles fire-and-forget invocation patterns with result storage/publishing
    pub async fn start_async_invoke_loop(
        &mut self,
        route: &FuncInvokeRoute,
        fn_id: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let async_key = format!("{}_async", fn_id);
        if self.async_subscriber_table.contains_key(&async_key) {
            return Ok(());
        }

        // Build the key expression for async invocations with wildcard for invocation_id
        let key = match route.stateless {
            true => format!("{}/invokes/{}/async/*", self.prefix, fn_id),
            false => {
                format!("{}/objects/*/invokes/{}/async/*", self.prefix, fn_id)
            }
        };

        let handler = AsyncInvocationHandler::new(
            format!("Shard {} Async", self.meta.id),
            self.offloader.clone(),
        );

        tracing::info!(
            "shard {}: declare async subscriber {}",
            self.meta.id,
            key
        );
        let config = ManagedConfig::new(key, 64, 65536);
        let subscriber =
            declare_managed_subscriber(&self.z_session, config, handler)
                .await?;
        self.async_subscriber_table.insert(async_key, subscriber);
        Ok(())
    }

    /// Handle liveliness updates for dynamic endpoint management
    /// This method starts/stops invocation endpoints based on shard liveliness
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
                    .read_sync(active_id, |_, v| *v)
                    .unwrap_or(false);
                should_active &= !live;
            }
            tracing::info!(
                "shard {}: invocation {} should be active: {should_active}, active group: {active_group:?}, liveliness: {:?}",
                self.meta.id,
                fn_id,
                state.liveliness_map
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
                if let Err(err) =
                    self.start_async_invoke_loop(route, fn_id).await
                {
                    tracing::error!(
                        "shard {}: failed to start async invoke loop for {}: {:?}",
                        self.meta.id,
                        fn_id,
                        err
                    );
                };
            } else {
                // Stop queryable for sync invocations
                let queryable = self.queryable_table.remove(fn_id);
                if let Some(q) = queryable {
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

                // Stop subscriber for async invocations
                let async_key = format!("{}_async", fn_id);
                let async_subscriber =
                    self.async_subscriber_table.remove(&async_key);
                if let Some(subscriber) = async_subscriber {
                    tracing::info!(
                        "shard {}: undeclare async invocation subscriber for {}",
                        self.meta.id,
                        fn_id
                    );
                    if let Err(e) = subscriber.undeclare().await {
                        tracing::error!(
                            "shard {}: failed to undeclare async subscriber {}: {:?}",
                            self.meta.id,
                            fn_id,
                            e
                        );
                    };
                }
            }
        }
    }

    /// Stop all invocation loops and clean up resources
    pub async fn stop(&mut self) {
        // Stop all synchronous invocation queryables
        let all_fn: Vec<String> =
            self.queryable_table.keys().cloned().collect();

        for fn_id in all_fn.iter() {
            if let Some(queryable) = self.queryable_table.remove(fn_id) {
                if let Err(e) = queryable.undeclare().await {
                    tracing::warn!("Failed to undeclare queryable: {:?}", e);
                };
            }
        }

        // Stop all asynchronous invocation subscribers
        let all_async_keys: Vec<String> =
            self.async_subscriber_table.keys().cloned().collect();

        for async_key in all_async_keys.iter() {
            if let Some(subscriber) =
                self.async_subscriber_table.remove(async_key)
            {
                if let Err(e) = subscriber.undeclare().await {
                    tracing::warn!(
                        "Failed to undeclare async subscriber: {:?}",
                        e
                    );
                };
            }
        }
    }
}
