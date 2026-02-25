//! Adapter bridging `WasmInvocationExecutor` to the `InvocationExecutor` trait.
//!
//! The `InvocationExecutor` trait uses protobuf types (`InvocationRequest`,
//! `ObjectInvocationRequest`, `InvocationResponse`). The WASM executor uses
//! WIT-generated types. This adapter converts between the two.

use std::sync::Arc;

use crate::executor::{WasmInvocationExecutor, WasmResponseStatus};
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
}

impl WasmExecutorAdapter {
    pub fn new(
        executor: Arc<WasmInvocationExecutor>,
        data_ops_factory: Arc<dyn DataOpsFactory>,
    ) -> Self {
        Self {
            executor,
            data_ops_factory,
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
            )
            .await
            .map_err(|e| {
                OffloadError::InternalError(format!("WASM invoke_fn failed: {}", e))
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
            )
            .await
            .map_err(|e| {
                OffloadError::InternalError(format!("WASM invoke_obj failed: {}", e))
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
        headers: result
            .headers
            .into_iter()
            .collect(),
        invocation_id: String::new(),
    }
}
