//! WASI Component Model runtime for OaaS WASM functions.
//!
//! This crate provides:
//! - WIT-generated bindings for the `oaas-function` world
//! - `WasmModuleStore` for fetching, compiling, and caching WASM components
//! - `WasmInvocationExecutor` implementing the `InvocationExecutor` trait
//! - Host function implementations bridging WASM guests to ODGM data operations

pub mod adapter;
pub mod executor;
pub mod host;
pub mod store;

// Make mock_ops conditionally compiled but public for tests.
// or just remove cfg entirely so integration tests can see it
pub mod mock_ops;

// Generate Rust bindings from WIT.
// This creates types and traits for the oaas-function world:
// - Host trait for data-access imports (implemented by WasmHostState)
// - Guest bindings for guest-function exports (invoke-fn, invoke-obj)
wasmtime::component::bindgen!({
    world: "oaas-function",
    path: "wit/oaas.wit",
    async: true,
});
