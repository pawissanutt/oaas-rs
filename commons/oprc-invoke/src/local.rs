//! Local (in-process) function invocation support.
//!
//! This module provides [`LocalFnOffloader`], a registry of Rust closures that
//! execute function calls in-process without external gRPC round-trips.  It is
//! the foundation layer for WASM runtime integration: a WASM engine (e.g.
//! `wasmtime`) can be wrapped in a closure and registered here, so that ODGM
//! can execute WebAssembly functions locally.
//!
//! # Example
//!
//! ```rust,ignore
//! use oprc_invoke::local::LocalFnOffloader;
//!
//! let mut offloader = LocalFnOffloader::new();
//! offloader.register_fn("echo", |payload| payload);
//!
//! // Attach to shard via shard.with_local_offloader(Arc::new(offloader))
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjectInvocationRequest,
    ResponseStatus,
};

use crate::OffloadError;
use crate::handler::InvocationExecutor;

/// A synchronous in-process function handler: receives a raw payload and
/// returns a raw response payload.
pub type FnHandlerFn = Arc<dyn Fn(Vec<u8>) -> Vec<u8> + Send + Sync>;

/// In-process function offloader backed by registered Rust closures.
///
/// Functions are keyed by their `fn_id`.  When `invoke_fn` or `invoke_obj` is
/// called, the matching closure is executed synchronously without a
/// `spawn_blocking` wrapper.  This is acceptable for fast, CPU-light handlers.
/// WASM-executing closures that may take longer should use
/// `tokio::task::spawn_blocking` internally, or wrap the call with a
/// dedicated async executor, to avoid blocking the Tokio runtime thread.
pub struct LocalFnOffloader {
    /// Handlers for stateless function invocations (`invoke_fn`).
    fn_handlers: HashMap<String, FnHandlerFn>,
    /// Handlers for object method invocations (`invoke_obj`).
    /// Falls back to `fn_handlers` when no dedicated handler is registered.
    obj_handlers: HashMap<String, FnHandlerFn>,
}

impl Default for LocalFnOffloader {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalFnOffloader {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            fn_handlers: HashMap::new(),
            obj_handlers: HashMap::new(),
        }
    }

    /// Register a stateless function handler.
    ///
    /// `handler` receives the raw request payload and must return the raw
    /// response payload.
    pub fn register_fn(
        &mut self,
        fn_id: impl Into<String>,
        handler: impl Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
    ) {
        self.fn_handlers.insert(fn_id.into(), Arc::new(handler));
    }

    /// Register an object method handler.
    ///
    /// If no object handler is registered for a given `fn_id`, [`invoke_obj`]
    /// falls back to the stateless handler registered via [`register_fn`].
    pub fn register_obj_fn(
        &mut self,
        fn_id: impl Into<String>,
        handler: impl Fn(Vec<u8>) -> Vec<u8> + Send + Sync + 'static,
    ) {
        self.obj_handlers.insert(fn_id.into(), Arc::new(handler));
    }

    /// Returns `true` if no handler is registered.
    pub fn is_empty(&self) -> bool {
        self.fn_handlers.is_empty() && self.obj_handlers.is_empty()
    }

    /// Returns `true` if a handler is registered for the given `fn_id`
    /// (either as a stateless or object handler).
    pub fn has_fn(&self, fn_id: &str) -> bool {
        self.fn_handlers.contains_key(fn_id)
            || self.obj_handlers.contains_key(fn_id)
    }
}

#[async_trait::async_trait]
impl InvocationExecutor for LocalFnOffloader {
    async fn invoke_fn(
        &self,
        req: InvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        let fn_id = &req.fn_id;
        match self.fn_handlers.get(fn_id) {
            Some(handler) => {
                let result = (handler)(req.payload);
                Ok(InvocationResponse {
                    payload: Some(result),
                    status: ResponseStatus::Okay as i32,
                    ..Default::default()
                })
            }
            None => {
                Err(OffloadError::NoFunc(req.cls_id.clone(), fn_id.clone()))
            }
        }
    }

    async fn invoke_obj(
        &self,
        req: ObjectInvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        let fn_id = &req.fn_id;
        // Prefer a dedicated object handler; fall back to stateless handler.
        let handler = self
            .obj_handlers
            .get(fn_id)
            .or_else(|| self.fn_handlers.get(fn_id));
        match handler {
            Some(h) => {
                let result = (h)(req.payload);
                Ok(InvocationResponse {
                    payload: Some(result),
                    status: ResponseStatus::Okay as i32,
                    ..Default::default()
                })
            }
            None => {
                Err(OffloadError::NoFunc(req.cls_id.clone(), fn_id.clone()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_invoke_fn_echo() {
        let mut offloader = LocalFnOffloader::new();
        offloader.register_fn("echo", |payload| payload);

        let req = InvocationRequest {
            cls_id: "test".to_string(),
            fn_id: "echo".to_string(),
            payload: b"hello".to_vec(),
            ..Default::default()
        };
        let resp = offloader.invoke_fn(req).await.unwrap();
        assert_eq!(resp.status, ResponseStatus::Okay as i32);
        assert_eq!(resp.payload, Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn test_invoke_fn_missing_returns_error() {
        let offloader = LocalFnOffloader::new();
        let req = InvocationRequest {
            cls_id: "test".to_string(),
            fn_id: "missing".to_string(),
            payload: vec![],
            ..Default::default()
        };
        let result = offloader.invoke_fn(req).await;
        assert!(
            matches!(result, Err(OffloadError::NoFunc(_, _))),
            "expected NoFunc error"
        );
    }

    #[tokio::test]
    async fn test_invoke_obj_falls_back_to_fn_handler() {
        let mut offloader = LocalFnOffloader::new();
        offloader.register_fn("process", |mut p| {
            p.extend_from_slice(b"-processed");
            p
        });

        let req = ObjectInvocationRequest {
            cls_id: "test".to_string(),
            fn_id: "process".to_string(),
            payload: b"data".to_vec(),
            ..Default::default()
        };
        let resp = offloader.invoke_obj(req).await.unwrap();
        assert_eq!(resp.status, ResponseStatus::Okay as i32);
        assert_eq!(resp.payload, Some(b"data-processed".to_vec()));
    }

    #[tokio::test]
    async fn test_invoke_obj_prefers_obj_handler() {
        let mut offloader = LocalFnOffloader::new();
        offloader.register_fn("fn", |_| b"from-fn".to_vec());
        offloader.register_obj_fn("fn", |_| b"from-obj".to_vec());

        let req = ObjectInvocationRequest {
            cls_id: "test".to_string(),
            fn_id: "fn".to_string(),
            payload: vec![],
            ..Default::default()
        };
        let resp = offloader.invoke_obj(req).await.unwrap();
        assert_eq!(resp.payload, Some(b"from-obj".to_vec()));
    }

    #[test]
    fn test_has_fn_and_is_empty() {
        let mut offloader = LocalFnOffloader::new();
        assert!(offloader.is_empty());
        assert!(!offloader.has_fn("echo"));

        offloader.register_fn("echo", |p| p);
        assert!(!offloader.is_empty());
        assert!(offloader.has_fn("echo"));
        assert!(!offloader.has_fn("other"));
    }
}
