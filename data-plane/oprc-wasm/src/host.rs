//! Host function implementations bridging WASM guests to ODGM data operations.

use async_trait::async_trait;

/// Trait abstracting ODGM data operations for the WASM host.
/// Implemented by `ShardDataOpsAdapter` in oprc-odgm to bridge to real shard ops.
#[async_trait]
pub trait OdgmDataOps: Send + Sync {
    async fn get_object(
        &self,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
    ) -> Result<Option<Vec<u8>>, DataOpsError>;

    async fn set_object(
        &self,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        data: Vec<u8>,
    ) -> Result<(), DataOpsError>;

    async fn delete_object(
        &self,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
    ) -> Result<(), DataOpsError>;

    async fn get_value(
        &self,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        key: &str,
    ) -> Result<Option<Vec<u8>>, DataOpsError>;

    async fn set_value(
        &self,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        key: &str,
        value: Vec<u8>,
    ) -> Result<(), DataOpsError>;

    async fn delete_value(
        &self,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        key: &str,
    ) -> Result<(), DataOpsError>;

    async fn invoke_fn(
        &self,
        cls_id: &str,
        partition_id: u32,
        fn_id: &str,
        payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, DataOpsError>;

    /// Invoke a method on a specific object (object-bound invocation).
    async fn invoke_obj(
        &self,
        cls_id: &str,
        partition_id: u32,
        object_id: &str,
        fn_id: &str,
        payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, DataOpsError>;
}

/// Errors returned by data operations.
#[derive(Debug, thiserror::Error)]
pub enum DataOpsError {
    #[error("not found")]
    NotFound,
    #[error("permission denied")]
    PermissionDenied,
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("internal: {0}")]
    Internal(String),
}

use wasmtime_wasi::ResourceTable;
use wasmtime_wasi::p2::{IoView, WasiCtx, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

/// WASM host state — held in wasmtime::Store, providing data-access implementations.
pub struct WasmHostState {
    pub data_ops: Box<dyn OdgmDataOps>,
    pub cls_id: String,
    pub partition_id: u32,
    pub object_id: Option<String>,
    pub ctx: WasiCtx,
    pub http_ctx: WasiHttpCtx,
    pub table: ResourceTable,
}

impl IoView for WasmHostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiView for WasmHostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl WasiHttpView for WasmHostState {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http_ctx
    }
}

impl WasmHostState {
    pub fn new(
        data_ops: Box<dyn OdgmDataOps>,
        cls_id: String,
        partition_id: u32,
        object_id: Option<String>,
        ctx: WasiCtx,
    ) -> Self {
        Self {
            data_ops,
            cls_id,
            partition_id,
            object_id,
            ctx,
            http_ctx: WasiHttpCtx::new(),
            table: ResourceTable::new(),
        }
    }
}

// ─── Bridge: generated data_access::Host → OdgmDataOps ────────

use crate::oaas::odgm::data_access;
use crate::oaas::odgm::types::{Entry, ObjData, OdgmError, ValData, ValType};

impl From<DataOpsError> for OdgmError {
    fn from(e: DataOpsError) -> Self {
        match e {
            DataOpsError::NotFound => OdgmError::NotFound,
            DataOpsError::PermissionDenied => OdgmError::PermissionDenied,
            DataOpsError::InvalidArgument(msg) => {
                OdgmError::InvalidArgument(msg)
            }
            DataOpsError::Internal(msg) => OdgmError::Internal(msg),
        }
    }
}

// The WIT types interface generates a marker Host trait
impl crate::oaas::odgm::types::Host for WasmHostState {}

fn bytes_to_obj(data: Vec<u8>) -> ObjData {
    ObjData {
        metadata: None,
        entries: vec![Entry {
            key: "_raw".to_string(),
            value: ValData {
                data,
                val_type: ValType::Byte,
            },
        }],
    }
}

fn obj_to_bytes(obj: &ObjData) -> Vec<u8> {
    obj.entries
        .first()
        .map(|e| e.value.data.clone())
        .unwrap_or_default()
}

// wasmtime bindgen! with `async: true` on Rust 2024 uses native async traits.
// We implement the trait directly without #[async_trait].
impl data_access::Host for WasmHostState {
    async fn get_object(
        &mut self,
        cls_id: String,
        partition_id: u32,
        object_id: String,
    ) -> Result<Option<ObjData>, OdgmError> {
        self.data_ops
            .get_object(&cls_id, partition_id, &object_id)
            .await
            .map(|opt| opt.map(bytes_to_obj))
            .map_err(OdgmError::from)
    }

    async fn set_object(
        &mut self,
        cls_id: String,
        partition_id: u32,
        object_id: String,
        obj: ObjData,
    ) -> Result<(), OdgmError> {
        self.data_ops
            .set_object(&cls_id, partition_id, &object_id, obj_to_bytes(&obj))
            .await
            .map_err(OdgmError::from)
    }

    async fn delete_object(
        &mut self,
        cls_id: String,
        partition_id: u32,
        object_id: String,
    ) -> Result<(), OdgmError> {
        self.data_ops
            .delete_object(&cls_id, partition_id, &object_id)
            .await
            .map_err(OdgmError::from)
    }

    async fn merge_object(
        &mut self,
        cls_id: String,
        partition_id: u32,
        object_id: String,
        obj: ObjData,
    ) -> Result<Option<ObjData>, OdgmError> {
        self.data_ops
            .set_object(&cls_id, partition_id, &object_id, obj_to_bytes(&obj))
            .await
            .map_err(OdgmError::from)?;
        self.get_object(cls_id, partition_id, object_id).await
    }

    async fn get_value(
        &mut self,
        cls_id: String,
        partition_id: u32,
        object_id: String,
        key: String,
    ) -> Result<Option<ValData>, OdgmError> {
        self.data_ops
            .get_value(&cls_id, partition_id, &object_id, &key)
            .await
            .map(|opt| {
                opt.map(|data| ValData {
                    data,
                    val_type: ValType::Byte,
                })
            })
            .map_err(OdgmError::from)
    }

    async fn set_value(
        &mut self,
        cls_id: String,
        partition_id: u32,
        object_id: String,
        key: String,
        value: ValData,
    ) -> Result<(), OdgmError> {
        self.data_ops
            .set_value(&cls_id, partition_id, &object_id, &key, value.data)
            .await
            .map_err(OdgmError::from)
    }

    async fn delete_value(
        &mut self,
        cls_id: String,
        partition_id: u32,
        object_id: String,
        key: String,
    ) -> Result<(), OdgmError> {
        self.data_ops
            .delete_value(&cls_id, partition_id, &object_id, &key)
            .await
            .map_err(OdgmError::from)
    }

    async fn invoke_fn(
        &mut self,
        cls_id: String,
        partition_id: u32,
        fn_id: String,
        payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, OdgmError> {
        self.data_ops
            .invoke_fn(&cls_id, partition_id, &fn_id, payload)
            .await
            .map_err(OdgmError::from)
    }
}
