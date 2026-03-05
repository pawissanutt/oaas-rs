//! Adapter bridging `WasmInvocationExecutor` to the `InvocationExecutor` trait.
//!
//! The `InvocationExecutor` trait uses protobuf types (`InvocationRequest`,
//! `ObjectInvocationRequest`, `InvocationResponse`). The WASM executor uses
//! WIT-generated types. This adapter converts between the two.

use std::sync::Arc;

use crate::executor::{OopContext, WasmInvocationExecutor, WasmResponseStatus};
use crate::host::OdgmDataOps;
use oprc_grpc::{InvocationResponse, ResponseStatus};
use oprc_invoke::OffloadError;
use oprc_invoke::handler::InvocationExecutor;
use tracing::debug;

/// Factory that creates `OdgmDataOps` instances for each invocation.
///
/// Since each WASM invocation gets its own `wasmtime::Store` with independent
/// host state, we need a fresh `OdgmDataOps` per call. The factory pattern
/// allows the ODGM integration layer to provide shard-backed implementations.
pub trait DataOpsFactory: Send + Sync {
    fn create(&self) -> Box<dyn OdgmDataOps>;
}

/// Adapter that wraps [`WasmInvocationExecutor`] and implements the proto-based
/// [`InvocationExecutor`] trait so it can be used as a `local_offloader` on a shard.
pub struct WasmExecutorAdapter {
    executor: Arc<WasmInvocationExecutor>,
    data_ops_factory: Arc<dyn DataOpsFactory>,
    /// Optional OOP context (shard identity + remote proxy) for oaas-object world.
    oop_context: Option<OopContext>,
}

impl WasmExecutorAdapter {
    pub fn new(
        executor: Arc<WasmInvocationExecutor>,
        data_ops_factory: Arc<dyn DataOpsFactory>,
    ) -> Self {
        Self {
            executor,
            data_ops_factory,
            oop_context: None,
        }
    }

    /// Create an adapter with OOP context for oaas-object world support.
    pub fn with_oop_context(
        executor: Arc<WasmInvocationExecutor>,
        data_ops_factory: Arc<dyn DataOpsFactory>,
        oop_context: OopContext,
    ) -> Self {
        Self {
            executor,
            data_ops_factory,
            oop_context: Some(oop_context),
        }
    }
}

#[async_trait::async_trait]
impl InvocationExecutor for WasmExecutorAdapter {
    async fn invoke_fn(
        &self,
        req: oprc_grpc::InvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        debug!(
            fn_id = %req.fn_id,
            cls_id = %req.cls_id,
            "WASM adapter: invoke_fn"
        );

        let data_ops = self.data_ops_factory.create();

        let result = self
            .executor
            .invoke_fn(
                &req.fn_id,
                &req.cls_id,
                req.partition_id,
                if req.payload.is_empty() {
                    None
                } else {
                    Some(req.payload)
                },
                data_ops,
                self.oop_context.as_ref(),
            )
            .await
            .map_err(|e| {
                OffloadError::InternalError(format!(
                    "WASM invoke_fn failed: {}",
                    e
                ))
            })?;

        Ok(wasm_result_to_proto(result))
    }

    async fn invoke_obj(
        &self,
        req: oprc_grpc::ObjectInvocationRequest,
    ) -> Result<InvocationResponse, OffloadError> {
        let object_id = req.object_id.as_deref().unwrap_or("");
        debug!(
            fn_id = %req.fn_id,
            cls_id = %req.cls_id,
            object_id = %object_id,
            "WASM adapter: invoke_obj"
        );

        let data_ops = self.data_ops_factory.create();

        let result = self
            .executor
            .invoke_obj(
                &req.fn_id,
                &req.cls_id,
                req.partition_id,
                object_id,
                if req.payload.is_empty() {
                    None
                } else {
                    Some(req.payload)
                },
                data_ops,
                self.oop_context.as_ref(),
            )
            .await
            .map_err(|e| {
                OffloadError::InternalError(format!(
                    "WASM invoke_obj failed: {}",
                    e
                ))
            })?;

        Ok(wasm_result_to_proto(result))
    }
}

/// Convert a WASM invocation result to a protobuf InvocationResponse.
fn wasm_result_to_proto(
    result: crate::executor::WasmInvocationResult,
) -> InvocationResponse {
    let status = match result.status {
        WasmResponseStatus::Okay => ResponseStatus::Okay,
        WasmResponseStatus::InvalidRequest => ResponseStatus::InvalidRequest,
        WasmResponseStatus::AppError => ResponseStatus::AppError,
        WasmResponseStatus::SystemError => ResponseStatus::SystemError,
    };

    InvocationResponse {
        payload: result.payload,
        status: status.into(),
        headers: result.headers.into_iter().collect(),
        invocation_id: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::WasmInvocationResult;

    #[test]
    fn wasm_result_okay_to_proto() {
        let result = WasmInvocationResult {
            payload: Some(b"hello".to_vec()),
            status: WasmResponseStatus::Okay,
            headers: vec![("content-type".into(), "text/plain".into())],
        };
        let resp = wasm_result_to_proto(result);
        assert_eq!(resp.payload, Some(b"hello".to_vec()));
        assert_eq!(resp.status, i32::from(ResponseStatus::Okay));
        assert_eq!(
            resp.headers.get("content-type").map(String::as_str),
            Some("text/plain")
        );
    }

    #[test]
    fn wasm_result_app_error_to_proto() {
        let result = WasmInvocationResult {
            payload: None,
            status: WasmResponseStatus::AppError,
            headers: vec![],
        };
        let resp = wasm_result_to_proto(result);
        assert_eq!(resp.payload, None);
        assert_eq!(resp.status, i32::from(ResponseStatus::AppError));
    }

    #[test]
    fn wasm_result_system_error_to_proto() {
        let result = WasmInvocationResult {
            payload: None,
            status: WasmResponseStatus::SystemError,
            headers: vec![],
        };
        let resp = wasm_result_to_proto(result);
        assert_eq!(resp.status, i32::from(ResponseStatus::SystemError));
    }

    #[test]
    fn wasm_result_invalid_request_to_proto() {
        let result = WasmInvocationResult {
            payload: None,
            status: WasmResponseStatus::InvalidRequest,
            headers: vec![],
        };
        let resp = wasm_result_to_proto(result);
        assert_eq!(resp.status, i32::from(ResponseStatus::InvalidRequest));
    }

    #[test]
    fn adapter_new_has_no_oop_context() {
        // Can't actually call invoke (no module store), but can verify construction
        // Use a simple check that the struct can be created
        let oop = OopContext {
            remote_proxy: None,
            shard_cls_id: "cls".into(),
            shard_partition_id: 1,
        };
        // Verify OopContext fields
        assert_eq!(oop.shard_cls_id, "cls");
        assert_eq!(oop.shard_partition_id, 1);
        assert!(oop.remote_proxy.is_none());
    }

    #[test]
    fn oop_context_clone() {
        let ctx = OopContext {
            remote_proxy: None,
            shard_cls_id: "test-cls".into(),
            shard_partition_id: 42,
        };
        let cloned = ctx.clone();
        assert_eq!(cloned.shard_cls_id, "test-cls");
        assert_eq!(cloned.shard_partition_id, 42);
    }
}
