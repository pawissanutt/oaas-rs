//! WasmInvocationExecutor — dispatches invocations to WASM components.

use crate::OaasFunction;
use crate::host::WasmHostState;
use crate::oaas::odgm::types::{
    InvocationRequest as WitRequest, InvocationResponse as WitResponse,
    ResponseStatus,
};
use crate::store::WasmModuleStore;
use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{debug, info};
use wasmtime::Store;
use wasmtime::component::Linker;

/// Executor that runs WASM component invocations in-process.
pub struct WasmInvocationExecutor {
    module_store: Arc<WasmModuleStore>,
    linker: Arc<Linker<WasmHostState>>,
}

impl WasmInvocationExecutor {
    pub fn new(module_store: Arc<WasmModuleStore>) -> Result<Self> {
        let engine = module_store.engine().clone();
        let mut linker = Linker::new(&engine);

        // Register WASI implementations
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;

        // Register host functions from the WIT data-access interface
        OaasFunction::add_to_linker(
            &mut linker,
            |state: &mut WasmHostState| state,
        )?;

        info!("WasmInvocationExecutor initialized with host functions linked");

        Ok(Self {
            module_store,
            linker: Arc::new(linker),
        })
    }

    /// Invoke a stateless function (InvokeFn).
    pub async fn invoke_fn(
        &self,
        fn_id: &str,
        cls_id: &str,
        partition_id: u32,
        payload: Option<Vec<u8>>,
        data_ops: Box<dyn crate::host::OdgmDataOps>,
    ) -> Result<WasmInvocationResult> {
        let module = self.module_store.get(fn_id).await.ok_or_else(|| {
            anyhow::anyhow!("WASM module not loaded for fn_id={}", fn_id)
        })?;

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

        // Configure fuel for CPU limits (prevent runaway guests)
        store.set_fuel(1_000_000_000)?; // ~1 billion fuel units

        let instance = OaasFunction::instantiate_async(
            &mut store,
            &module.component,
            &self.linker,
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
    pub async fn invoke_obj(
        &self,
        fn_id: &str,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        payload: Option<Vec<u8>>,
        data_ops: Box<dyn crate::host::OdgmDataOps>,
    ) -> Result<WasmInvocationResult> {
        let module = self.module_store.get(fn_id).await.ok_or_else(|| {
            anyhow::anyhow!("WASM module not loaded for fn_id={}", fn_id)
        })?;

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
            &module.component,
            &self.linker,
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
