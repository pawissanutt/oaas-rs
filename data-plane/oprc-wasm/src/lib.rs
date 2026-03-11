//! WASI Component Model runtime for OaaS WASM functions.
//!
//! This crate provides:
//! - WIT-generated bindings for the `oaas-function` and `oaas-object` worlds
//! - `WasmModuleStore` for fetching, compiling, and caching WASM components
//! - `WasmInvocationExecutor` implementing the `InvocationExecutor` trait
//! - Host function implementations bridging WASM guests to ODGM data operations

pub mod adapter;
pub mod executor;
pub mod host;
pub mod object_host;
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
    imports: {
        default: async,
    },
    exports: {
        default: async,
    },
});

// Generate Rust bindings for the OOP oaas-object world.
// This creates types and traits for:
// - object-context imports: object-proxy resource, object/object-by-str/log functions
// - guest-object exports: on-invoke
pub mod oaas_object_world {
    wasmtime::component::bindgen!({
        world: "oaas-object",
        path: "../oprc-wasm/wit/oaas.wit",
        imports: {
            default: async,
        },
        exports: {
            default: async,
        },
        with: {
            // Share types between the two bindgen outputs
            "oaas:odgm/types": crate::oaas::odgm::types,
            // Map the WIT resource type to our host-side state struct
            // so that ResourceTable operations use ObjectProxyState directly.
            "oaas:odgm/object-context.object-proxy": crate::object_host::ObjectProxyState,
        },
    });
}
