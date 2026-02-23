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

/// WASM host state — held in wasmtime::Store, providing data-access implementations.
pub struct WasmHostState {
    /// Data operations backend (connects to ODGM shard)
    pub data_ops: Box<dyn OdgmDataOps>,
    /// Invocation context: which class/partition this function is scoped to
    pub cls_id: String,
    pub partition_id: u32,
    /// Optional object_id for object method invocations
    pub object_id: Option<String>,
}

impl WasmHostState {
    pub fn new(
        data_ops: Box<dyn OdgmDataOps>,
        cls_id: String,
        partition_id: u32,
        object_id: Option<String>,
    ) -> Self {
        Self {
            data_ops,
            cls_id,
            partition_id,
            object_id,
        }
    }
}
