//! WasmInvocationExecutor — dispatches invocations to WASM components.

use crate::host::WasmHostState;
use crate::store::WasmModuleStore;
use anyhow::Result;
use std::sync::Arc;
use wasmtime::component::Linker;

/// Executor that runs WASM component invocations in-process.
pub struct WasmInvocationExecutor {
    store: Arc<WasmModuleStore>,
    #[allow(dead_code)]
    linker: Linker<WasmHostState>,
}

impl WasmInvocationExecutor {
    pub fn new(module_store: Arc<WasmModuleStore>) -> Result<Self> {
        let engine = module_store.engine().clone();
        let linker = Linker::new(&engine);

        // TODO: Link host functions (data-access interface) to the linker
        // This will be done when we implement the generated Host trait

        Ok(Self {
            store: module_store,
            linker,
        })
    }

    /// Get a reference to the module store.
    pub fn module_store(&self) -> &WasmModuleStore {
        &self.store
    }
}
