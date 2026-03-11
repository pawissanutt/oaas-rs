//! Bridge between ODGM shard operations and the WASM runtime.
//!
//! This module provides:
//! - [`ShardDataOpsAdapter`]: implements `OdgmDataOps` by delegating to an `ObjectShard`
//! - [`ShardDataOpsFactory`]: implements `DataOpsFactory` to produce adapters per call
//! - [`setup_wasm_offloader`]: detects `wasm://` routes and builds the WASM executor

use std::sync::Arc;

use oprc_wasm::adapter::{DataOpsFactory, WasmExecutorAdapter};
use oprc_wasm::executor::{OopContext, WasmInvocationExecutor};
use oprc_wasm::host::{DataOpsError, OdgmDataOps};
use oprc_wasm::store::WasmModuleStore;

use crate::shard::object_trait::ArcUnifiedObjectShard;
use crate::shard::{ObjectVal, ShardMetadata};
use oprc_grpc::InvocationRequest;
use tracing::{debug, info, warn};

// ─── ShardDataOpsAdapter ────────────────────────────────────

/// Adapter implementing `OdgmDataOps` by delegating to a dyn `ObjectShard`.
///
/// Each WASM invocation receives its own `OdgmDataOps` instance. The adapter
/// wraps an `Arc<dyn ObjectShard>` so all invocations share the same underlying
/// shard (thread-safe).
pub struct ShardDataOpsAdapter {
    shard: ArcUnifiedObjectShard,
}

impl ShardDataOpsAdapter {
    pub fn new(shard: ArcUnifiedObjectShard) -> Self {
        Self { shard }
    }
}

fn shard_err_to_ops(e: crate::shard::ShardError) -> DataOpsError {
    DataOpsError::Internal(e.to_string())
}

/// The conventional entry key used for raw object data in WASM operations.
const RAW_ENTRY_KEY: &str = "_raw";

#[async_trait::async_trait]
impl OdgmDataOps for ShardDataOpsAdapter {
    async fn get_object(
        &self,
        _cls_id: &str,
        _partition_id: u32,
        object_id: &str,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        // Return the `_raw` entry's data as the object bytes.
        // This matches the convention used by the WIT host layer (bytes_to_obj/obj_to_bytes).
        match self
            .shard
            .get_entry_granular(object_id, RAW_ENTRY_KEY)
            .await
        {
            Ok(Some(val)) => Ok(Some(val.data)),
            Ok(None) => Ok(None),
            Err(e) => Err(shard_err_to_ops(e)),
        }
    }

    async fn set_object(
        &self,
        _cls_id: &str,
        _partition_id: u32,
        object_id: &str,
        data: Vec<u8>,
    ) -> Result<(), DataOpsError> {
        // Ensure the object metadata exists first
        let _ = self.shard.ensure_metadata_exists(object_id).await;
        let val = ObjectVal {
            data,
            r#type: oprc_grpc::ValType::Byte,
        };
        self.shard
            .set_entry_granular(object_id, RAW_ENTRY_KEY, val)
            .await
            .map_err(shard_err_to_ops)
    }

    async fn delete_object(
        &self,
        _cls_id: &str,
        _partition_id: u32,
        object_id: &str,
    ) -> Result<(), DataOpsError> {
        self.shard
            .delete_object(object_id)
            .await
            .map_err(shard_err_to_ops)
    }

    async fn get_value(
        &self,
        _cls_id: &str,
        _partition_id: u32,
        object_id: &str,
        key: &str,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        match self.shard.get_entry_granular(object_id, key).await {
            Ok(Some(val)) => Ok(Some(val.data)),
            Ok(None) => Ok(None),
            Err(e) => Err(shard_err_to_ops(e)),
        }
    }

    async fn set_value(
        &self,
        _cls_id: &str,
        _partition_id: u32,
        object_id: &str,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), DataOpsError> {
        let val = ObjectVal {
            data: value,
            r#type: oprc_grpc::ValType::Byte,
        };
        self.shard
            .set_entry_granular(object_id, key, val)
            .await
            .map_err(shard_err_to_ops)
    }

    async fn delete_value(
        &self,
        _cls_id: &str,
        _partition_id: u32,
        object_id: &str,
        key: &str,
    ) -> Result<(), DataOpsError> {
        self.shard
            .delete_entry_granular(object_id, key)
            .await
            .map_err(shard_err_to_ops)
    }

    async fn invoke_fn(
        &self,
        _cls_id: &str,
        partition_id: u32,
        fn_id: &str,
        payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        let req = InvocationRequest {
            partition_id,
            cls_id: self.shard.meta().collection.clone(),
            fn_id: fn_id.to_string(),
            options: Default::default(),
            payload: payload.unwrap_or_default(),
        };
        match self.shard.invoke_fn(req).await {
            Ok(resp) => Ok(resp.payload),
            Err(e) => Err(DataOpsError::Internal(format!(
                "cross-invoke failed: {}",
                e
            ))),
        }
    }

    async fn invoke_obj(
        &self,
        _cls_id: &str,
        partition_id: u32,
        object_id: &str,
        fn_id: &str,
        payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        let req = oprc_grpc::ObjectInvocationRequest {
            partition_id,
            cls_id: self.shard.meta().collection.clone(),
            fn_id: fn_id.to_string(),
            object_id: Some(object_id.to_string()),
            options: Default::default(),
            payload: payload.unwrap_or_default(),
        };
        match self.shard.invoke_obj(req).await {
            Ok(resp) => Ok(resp.payload),
            Err(e) => Err(DataOpsError::Internal(format!(
                "cross-invoke-obj failed: {}",
                e
            ))),
        }
    }
}

// ─── DataOpsFactory ─────────────────────────────────────────

/// Factory producing `ShardDataOpsAdapter` instances for each WASM invocation.
pub struct ShardDataOpsFactory {
    shard: ArcUnifiedObjectShard,
}

impl ShardDataOpsFactory {
    pub fn new(shard: ArcUnifiedObjectShard) -> Self {
        Self { shard }
    }
}

impl DataOpsFactory for ShardDataOpsFactory {
    fn create(&self) -> Box<dyn OdgmDataOps> {
        Box::new(ShardDataOpsAdapter::new(self.shard.clone()))
    }
}

// ─── Setup helper ───────────────────────────────────────────

/// Inspect the shard's function routes for `wasm://` entries and, if any exist,
/// build a `WasmExecutorAdapter` suitable for use as `local_offloader`.
///
/// Returns `None` if the metadata has no `wasm://` routes.
pub async fn setup_wasm_offloader(
    metadata: &ShardMetadata,
    shard: ArcUnifiedObjectShard,
) -> Option<Arc<dyn oprc_invoke::handler::InvocationExecutor + Send + Sync>> {
    let wasm_routes: Vec<_> = metadata
        .invocations
        .fn_routes
        .iter()
        .filter(|(_, route)| route.url.starts_with("wasm://"))
        .collect();

    if wasm_routes.is_empty() {
        return None;
    }

    info!(
        collection = %metadata.collection,
        count = wasm_routes.len(),
        "Detected wasm:// function routes, initializing WASM runtime"
    );

    // Create wasmtime engine
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    config.consume_fuel(true);

    let engine = match wasmtime::Engine::new(&config) {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to create wasmtime engine: {}", e);
            return None;
        }
    };

    let module_store = Arc::new(WasmModuleStore::new(engine));

    // Load each WASM module
    for (fn_id, route) in &wasm_routes {
        let module_url = match &route.wasm_module_url {
            Some(url) => url.clone(),
            None => {
                warn!(
                    fn_id = %fn_id,
                    url = %route.url,
                    "wasm:// route missing wasm_module_url, skipping"
                );
                continue;
            }
        };

        debug!(fn_id = %fn_id, url = %module_url, "Loading WASM module");
        if let Err(e) = module_store.load(fn_id, &module_url).await {
            warn!(
                fn_id = %fn_id,
                url = %module_url,
                error = %e,
                "Failed to load WASM module"
            );
        }
    }

    let executor = match WasmInvocationExecutor::new(module_store) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            warn!("Failed to create WasmInvocationExecutor: {}", e);
            return None;
        }
    };

    let factory = Arc::new(ShardDataOpsFactory::new(shard));

    // Create OOP context for oaas-object world support.
    // Uses shard identity for locality checks; remote proxy is None for now
    // (can be injected when cross-shard Zenoh proxy is available).
    let oop_context = OopContext {
        remote_proxy: None,
        shard_cls_id: metadata.collection.clone(),
        shard_partition_id: metadata.partition_id as u32,
    };
    let adapter =
        WasmExecutorAdapter::with_oop_context(executor, factory, oop_context);

    Some(Arc::new(adapter))
}
