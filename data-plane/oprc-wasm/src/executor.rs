//! WasmInvocationExecutor — dispatches invocations to WASM components.

use crate::OaasFunction;
use crate::host::WasmHostState;
use crate::oaas::odgm::types::{
    InvocationRequest as WitRequest, InvocationResponse as WitResponse,
    ObjectRef, ResponseStatus,
};
use crate::oaas_object_world::OaasObject;
use crate::object_host::ObjectWasmHostState;
use crate::store::{WasmModuleStore, WorldType};
use anyhow::{Context, Result};
use oprc_invoke::proxy::ObjectProxy as ZenohObjectProxy;
use std::sync::Arc;
use tracing::{debug, info};
use wasmtime::Store;
use wasmtime::component::Linker;

/// Context required for object-oriented (oaas-object) world invocations.
///
/// Carries shard identity (for locality checks) and an optional remote
/// Zenoh proxy for cross-shard data/invocation operations.
#[derive(Clone)]
pub struct OopContext {
    pub remote_proxy: Option<Arc<ZenohObjectProxy>>,
    pub shard_cls_id: String,
    pub shard_partition_id: u32,
}

/// Executor that runs WASM component invocations in-process.
///
/// Maintains two linkers: one for the procedural `oaas-function` world and
/// one for the OOP `oaas-object` world. At invocation time, the executor
/// dispatches to the correct linker based on the compiled module's world type.
pub struct WasmInvocationExecutor {
    module_store: Arc<WasmModuleStore>,
    /// Linker for the procedural `oaas-function` world.
    fn_linker: Arc<Linker<WasmHostState>>,
    /// Linker for the OOP `oaas-object` world.
    obj_linker: Arc<Linker<ObjectWasmHostState>>,
}

impl WasmInvocationExecutor {
    pub fn new(module_store: Arc<WasmModuleStore>) -> Result<Self> {
        let engine = module_store.engine().clone();

        // ── Linker for oaas-function (procedural) world ──
        let mut fn_linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut fn_linker)?;
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut fn_linker)?;
        OaasFunction::add_to_linker(
            &mut fn_linker,
            |state: &mut WasmHostState| state,
        )?;

        // ── Linker for oaas-object (OOP) world ──
        let mut obj_linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut obj_linker)?;
        wasmtime_wasi_http::add_only_http_to_linker_async(&mut obj_linker)?;
        OaasObject::add_to_linker(
            &mut obj_linker,
            |state: &mut ObjectWasmHostState| state,
        )?;

        info!("WasmInvocationExecutor initialized with dual-world linkers");

        Ok(Self {
            module_store,
            fn_linker: Arc::new(fn_linker),
            obj_linker: Arc::new(obj_linker),
        })
    }

    /// Invoke a stateless function (InvokeFn).
    ///
    /// For `Legacy` modules, calls `guest-function.invoke-fn`.
    /// For `ObjectOriented` modules, creates a self-proxy and calls
    /// `guest-object.on-invoke` with an empty object-id.
    pub async fn invoke_fn(
        &self,
        fn_id: &str,
        cls_id: &str,
        partition_id: u32,
        payload: Option<Vec<u8>>,
        data_ops: Box<dyn crate::host::OdgmDataOps>,
        oop_ctx: Option<&OopContext>,
    ) -> Result<WasmInvocationResult> {
        let module = self.module_store.get(fn_id).await.ok_or_else(|| {
            anyhow::anyhow!("WASM module not loaded for fn_id={}", fn_id)
        })?;

        match module.world_type {
            WorldType::Legacy => {
                self.invoke_fn_legacy(
                    fn_id,
                    cls_id,
                    partition_id,
                    payload,
                    data_ops,
                    &module.component,
                )
                .await
            }
            WorldType::ObjectOriented => {
                // For stateless OOP calls, use empty object_id
                self.invoke_oop(
                    fn_id,
                    cls_id,
                    partition_id,
                    "",
                    payload,
                    data_ops,
                    oop_ctx,
                    &module.component,
                )
                .await
            }
        }
    }

    /// Legacy invoke_fn: procedural world dispatch.
    async fn invoke_fn_legacy(
        &self,
        fn_id: &str,
        cls_id: &str,
        partition_id: u32,
        payload: Option<Vec<u8>>,
        data_ops: Box<dyn crate::host::OdgmDataOps>,
        component: &wasmtime::component::Component,
    ) -> Result<WasmInvocationResult> {
        let ctx = wasmtime_wasi::p2::WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();
        let host_state = WasmHostState::new(
            data_ops,
            cls_id.to_string(),
            partition_id,
            None,
            ctx,
        );

        let mut store = Store::new(self.module_store.engine(), host_state);
        store.set_fuel(1_000_000_000)?;

        let instance = OaasFunction::instantiate_async(
            &mut store,
            component,
            &self.fn_linker,
        )
        .await
        .context("failed to instantiate WASM component")?;

        let wit_req = WitRequest {
            partition_id,
            cls_id: cls_id.to_string(),
            fn_id: fn_id.to_string(),
            object_id: None,
            options: vec![],
            payload,
        };

        debug!(fn_id, cls_id, "invoking WASM guest function (stateless)");
        let response = instance
            .oaas_odgm_guest_function()
            .call_invoke_fn(&mut store, &wit_req)
            .await
            .context("WASM invoke_fn call failed")?;

        Ok(WasmInvocationResult::from_wit(response))
    }

    /// Invoke an object method (InvokeObj).
    ///
    /// For `Legacy` modules, calls `guest-function.invoke-obj`.
    /// For `ObjectOriented` modules, creates a self-proxy and calls
    /// `guest-object.on-invoke`.
    pub async fn invoke_obj(
        &self,
        fn_id: &str,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        payload: Option<Vec<u8>>,
        data_ops: Box<dyn crate::host::OdgmDataOps>,
        oop_ctx: Option<&OopContext>,
    ) -> Result<WasmInvocationResult> {
        let module = self.module_store.get(fn_id).await.ok_or_else(|| {
            anyhow::anyhow!("WASM module not loaded for fn_id={}", fn_id)
        })?;

        match module.world_type {
            WorldType::Legacy => {
                self.invoke_obj_legacy(
                    fn_id,
                    cls_id,
                    partition_id,
                    object_id,
                    payload,
                    data_ops,
                    &module.component,
                )
                .await
            }
            WorldType::ObjectOriented => {
                self.invoke_oop(
                    fn_id,
                    cls_id,
                    partition_id,
                    object_id,
                    payload,
                    data_ops,
                    oop_ctx,
                    &module.component,
                )
                .await
            }
        }
    }

    /// Legacy invoke_obj: procedural world dispatch.
    async fn invoke_obj_legacy(
        &self,
        fn_id: &str,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        payload: Option<Vec<u8>>,
        data_ops: Box<dyn crate::host::OdgmDataOps>,
        component: &wasmtime::component::Component,
    ) -> Result<WasmInvocationResult> {
        let ctx = wasmtime_wasi::p2::WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();
        let host_state = WasmHostState::new(
            data_ops,
            cls_id.to_string(),
            partition_id,
            Some(object_id.to_string()),
            ctx,
        );

        let mut store = Store::new(self.module_store.engine(), host_state);
        store.set_fuel(1_000_000_000)?;

        let instance = OaasFunction::instantiate_async(
            &mut store,
            component,
            &self.fn_linker,
        )
        .await
        .context("failed to instantiate WASM component")?;

        let wit_req = WitRequest {
            partition_id,
            cls_id: cls_id.to_string(),
            fn_id: fn_id.to_string(),
            object_id: Some(object_id.to_string()),
            options: vec![],
            payload,
        };

        debug!(
            fn_id,
            cls_id, object_id, "invoking WASM guest function (object method)"
        );
        let response = instance
            .oaas_odgm_guest_function()
            .call_invoke_obj(&mut store, &wit_req)
            .await
            .context("WASM invoke_obj call failed")?;

        Ok(WasmInvocationResult::from_wit(response))
    }

    /// OOP dispatch: create self-proxy, call guest-object.on-invoke.
    async fn invoke_oop(
        &self,
        fn_id: &str,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        payload: Option<Vec<u8>>,
        data_ops: Box<dyn crate::host::OdgmDataOps>,
        oop_ctx: Option<&OopContext>,
        component: &wasmtime::component::Component,
    ) -> Result<WasmInvocationResult> {
        let ctx = wasmtime_wasi::p2::WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();

        // Build OOP host state with shard identity + optional remote proxy
        let (remote_proxy, shard_cls, shard_part) = match oop_ctx {
            Some(c) => (
                c.remote_proxy.clone(),
                c.shard_cls_id.clone(),
                c.shard_partition_id,
            ),
            None => (None, cls_id.to_string(), partition_id),
        };

        let mut host_state = ObjectWasmHostState::new(
            data_ops,
            remote_proxy,
            shard_cls,
            shard_part,
            ctx,
        );

        // Create self-proxy in the resource table before moving into Store
        let self_ref = ObjectRef {
            cls: cls_id.to_string(),
            partition_id,
            object_id: object_id.to_string(),
        };
        let self_proxy = host_state.create_proxy(self_ref).map_err(|e| {
            anyhow::anyhow!("failed to create self proxy: {:?}", e)
        })?;

        let mut store = Store::new(self.module_store.engine(), host_state);
        store.set_fuel(1_000_000_000)?;

        let instance = OaasObject::instantiate_async(
            &mut store,
            component,
            &self.obj_linker,
        )
        .await
        .context("failed to instantiate OOP WASM component")?;

        debug!(
            fn_id,
            cls_id, object_id, "invoking OOP WASM guest (on-invoke)"
        );
        let response = instance
            .oaas_odgm_guest_object()
            .call_on_invoke(
                &mut store,
                self_proxy,
                fn_id,
                payload.as_deref(),
                &[],
            )
            .await
            .context("WASM on-invoke call failed")?;

        Ok(WasmInvocationResult::from_wit(response))
    }

    /// Get a reference to the module store.
    pub fn module_store(&self) -> &WasmModuleStore {
        &self.module_store
    }
}

/// Result of a WASM invocation in a Rust-friendly format.
#[derive(Debug)]
pub struct WasmInvocationResult {
    pub payload: Option<Vec<u8>>,
    pub status: WasmResponseStatus,
    pub headers: Vec<(String, String)>,
}

#[derive(Debug, PartialEq)]
pub enum WasmResponseStatus {
    Okay,
    InvalidRequest,
    AppError,
    SystemError,
}

impl WasmInvocationResult {
    fn from_wit(resp: WitResponse) -> Self {
        Self {
            payload: resp.payload,
            status: match resp.status {
                ResponseStatus::Okay => WasmResponseStatus::Okay,
                ResponseStatus::InvalidRequest => {
                    WasmResponseStatus::InvalidRequest
                }
                ResponseStatus::AppError => WasmResponseStatus::AppError,
                ResponseStatus::SystemError => WasmResponseStatus::SystemError,
            },
            headers: resp
                .headers
                .into_iter()
                .map(|kv| (kv.key, kv.value))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_ops::MockDataOps;
    use crate::store::WasmModuleStore;

    fn test_engine() -> Engine {
        let mut config = wasmtime::Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        config.consume_fuel(true);
        Engine::new(&config).unwrap()
    }

    use wasmtime::Engine;

    #[tokio::test]
    async fn invoke_fn_module_not_found() {
        let store = Arc::new(WasmModuleStore::new(test_engine()));
        let executor = WasmInvocationExecutor::new(store).unwrap();
        let mock = Box::new(MockDataOps::default());

        let result = executor
            .invoke_fn("nonexistent", "cls", 0, None, mock, None)
            .await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not loaded"));
    }

    #[tokio::test]
    async fn invoke_obj_module_not_found() {
        let store = Arc::new(WasmModuleStore::new(test_engine()));
        let executor = WasmInvocationExecutor::new(store).unwrap();
        let mock = Box::new(MockDataOps::default());

        let result = executor
            .invoke_obj("nonexistent", "cls", 0, "obj", None, mock, None)
            .await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not loaded"));
    }

    #[test]
    fn from_wit_okay() {
        let resp = WitResponse {
            payload: Some(b"data".to_vec()),
            status: ResponseStatus::Okay,
            headers: vec![],
        };
        let result = WasmInvocationResult::from_wit(resp);
        assert_eq!(result.status, WasmResponseStatus::Okay);
        assert_eq!(result.payload, Some(b"data".to_vec()));
        assert!(result.headers.is_empty());
    }

    #[test]
    fn from_wit_with_headers() {
        let resp = WitResponse {
            payload: None,
            status: ResponseStatus::AppError,
            headers: vec![crate::oaas::odgm::types::KeyValue {
                key: "x-trace".into(),
                value: "abc".into(),
            }],
        };
        let result = WasmInvocationResult::from_wit(resp);
        assert_eq!(result.status, WasmResponseStatus::AppError);
        assert_eq!(result.headers.len(), 1);
        assert_eq!(
            result.headers[0],
            ("x-trace".to_string(), "abc".to_string())
        );
    }

    #[test]
    fn from_wit_all_statuses() {
        for (wit_status, expected) in [
            (ResponseStatus::Okay, WasmResponseStatus::Okay),
            (
                ResponseStatus::InvalidRequest,
                WasmResponseStatus::InvalidRequest,
            ),
            (ResponseStatus::AppError, WasmResponseStatus::AppError),
            (ResponseStatus::SystemError, WasmResponseStatus::SystemError),
        ] {
            let resp = WitResponse {
                payload: None,
                status: wit_status,
                headers: vec![],
            };
            assert_eq!(WasmInvocationResult::from_wit(resp).status, expected);
        }
    }

    #[test]
    fn oop_context_defaults() {
        let ctx = OopContext {
            remote_proxy: None,
            shard_cls_id: "test".into(),
            shard_partition_id: 0,
        };
        assert!(ctx.remote_proxy.is_none());
        assert_eq!(ctx.shard_cls_id, "test");
        assert_eq!(ctx.shard_partition_id, 0);
    }
}
