//! Host implementation for the `oaas-object` world.
//!
//! Provides:
//! - [`ObjectProxyState`]: host-side state backing each `object-proxy` WIT resource
//! - [`ObjectWasmHostState`]: wasmtime Store data for the OOP world
//! - Host trait implementations for `object-context` interface (proxy methods + context functions)

use std::sync::Arc;

use oprc_grpc::ObjMeta;
use oprc_invoke::proxy::ObjectProxy as ZenohObjectProxy;
use tracing::{debug, error, info, warn};
use wasmtime::component::Resource;
use wasmtime_wasi::ResourceTable;
use wasmtime_wasi::p2::{IoView, WasiCtx, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

use crate::host::OdgmDataOps;
use crate::oaas::odgm::types::{
    Entry, FieldEntry, LogLevel, ObjData, ObjectRef, OdgmError, ValData,
    ValType,
};

use crate::oaas_object_world::oaas::odgm::object_context;

// ─── ObjectProxyState ───────────────────────────────────────

/// Host-side backing state for a single `object-proxy` WIT resource.
///
/// Each proxy is bound to a specific `(cls, partition, object_id)` triple.
/// It knows whether the target object is local (same shard) or remote,
/// and dispatches data operations accordingly.
pub struct ObjectProxyState {
    /// The identity of the object this proxy refers to.
    pub object_ref: ObjectRef,
    /// Whether this proxy targets the local shard.
    pub is_local: bool,
}

impl ObjectProxyState {
    pub fn new(object_ref: ObjectRef, is_local: bool) -> Self {
        Self {
            object_ref,
            is_local,
        }
    }

    /// Create an `ObjMeta` protobuf type for Zenoh proxy calls.
    #[allow(dead_code)]
    fn to_obj_meta(&self) -> ObjMeta {
        ObjMeta {
            cls_id: self.object_ref.cls.clone(),
            partition_id: self.object_ref.partition_id,
            object_id: Some(self.object_ref.object_id.clone()),
        }
    }
}

// ─── ObjectWasmHostState ────────────────────────────────────

/// Default maximum re-entrant invocation depth.
const DEFAULT_MAX_REENTRANT_DEPTH: u32 = 4;

/// WASM host state for the `oaas-object` world.
///
/// Held in `wasmtime::Store<ObjectWasmHostState>`. Provides:
/// - Local data operations via `OdgmDataOps`
/// - Remote data operations via Zenoh `ObjectProxy`
/// - Resource table for `object-proxy` handle lifecycle
/// - Re-entrancy depth tracking
pub struct ObjectWasmHostState {
    /// Local shard data operations (used for proxies targeting the same shard).
    pub data_ops: Box<dyn OdgmDataOps>,

    /// Remote object proxy for cross-shard operations via Zenoh.
    pub remote_proxy: Option<Arc<ZenohObjectProxy>>,

    /// The class of the current shard (for locality checks).
    pub shard_cls_id: String,

    /// The partition of the current shard (for locality checks).
    pub shard_partition_id: u32,

    /// WASI context.
    pub ctx: WasiCtx,

    /// WASI HTTP context (required by ComponentizeJS-produced guests).
    pub http_ctx: WasiHttpCtx,

    /// Resource table managing `object-proxy` handles.
    pub table: ResourceTable,

    /// Current re-entrant invocation depth.
    pub reentrant_depth: u32,

    /// Maximum allowed re-entrant depth.
    pub max_reentrant_depth: u32,
}

impl ObjectWasmHostState {
    pub fn new(
        data_ops: Box<dyn OdgmDataOps>,
        remote_proxy: Option<Arc<ZenohObjectProxy>>,
        shard_cls_id: String,
        shard_partition_id: u32,
        ctx: WasiCtx,
    ) -> Self {
        Self {
            data_ops,
            remote_proxy,
            shard_cls_id,
            shard_partition_id,
            ctx,
            http_ctx: WasiHttpCtx::new(),
            table: ResourceTable::new(),
            reentrant_depth: 0,
            max_reentrant_depth: DEFAULT_MAX_REENTRANT_DEPTH,
        }
    }

    /// Check whether an object-ref targets the local shard.
    fn is_local(&self, obj_ref: &ObjectRef) -> bool {
        obj_ref.cls == self.shard_cls_id
            && obj_ref.partition_id == self.shard_partition_id
    }

    /// Get the ObjectProxyState from the resource table by handle.
    fn get_proxy(
        &self,
        handle: &Resource<ObjectProxyState>,
    ) -> Result<&ObjectProxyState, OdgmError> {
        self.table.get(handle).map_err(|e| {
            OdgmError::Internal(format!("invalid proxy handle: {}", e))
        })
    }

    /// Create a proxy for the given object-ref, determining locality.
    pub(crate) fn create_proxy(
        &mut self,
        obj_ref: ObjectRef,
    ) -> Result<Resource<ObjectProxyState>, OdgmError> {
        let is_local = self.is_local(&obj_ref);
        let state = ObjectProxyState::new(obj_ref, is_local);
        self.table.push(state).map_err(|e| {
            OdgmError::Internal(format!("failed to create proxy: {}", e))
        })
    }

    /// Get the remote Zenoh proxy, returning an error if not available.
    fn remote(&self) -> Result<&ZenohObjectProxy, OdgmError> {
        self.remote_proxy.as_deref().ok_or_else(|| {
            OdgmError::Internal("remote proxy not available".into())
        })
    }
}

impl IoView for ObjectWasmHostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

impl WasiView for ObjectWasmHostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl WasiHttpView for ObjectWasmHostState {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http_ctx
    }
}

// Share the types interface marker trait
impl crate::oaas::odgm::types::Host for ObjectWasmHostState {}

// ─── Helper conversions ─────────────────────────────────────

fn convert_proxy_error(e: oprc_invoke::proxy::ProxyError) -> OdgmError {
    match &e {
        oprc_invoke::proxy::ProxyError::NoQueryable(_) => OdgmError::NotFound,
        _ => OdgmError::Internal(e.to_string()),
    }
}

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

// ─── HostObjectProxy implementation ─────────────────────────

impl object_context::HostObjectProxy for ObjectWasmHostState {
    /// Return the identity of this object.
    async fn ref_(&mut self, self_: Resource<ObjectProxyState>) -> ObjectRef {
        match self.get_proxy(&self_) {
            Ok(proxy) => proxy.object_ref.clone(),
            Err(_) => ObjectRef {
                cls: String::new(),
                partition_id: 0,
                object_id: String::new(),
            },
        }
    }

    /// Read a single field by key.
    async fn get(
        &mut self,
        self_: Resource<ObjectProxyState>,
        key: String,
    ) -> Result<Option<Vec<u8>>, OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        if is_local {
            self.data_ops
                .get_value(
                    &obj_ref.cls,
                    obj_ref.partition_id,
                    &obj_ref.object_id,
                    &key,
                )
                .await
                .map_err(OdgmError::from)
        } else {
            let meta = ObjMeta {
                cls_id: obj_ref.cls.clone(),
                partition_id: obj_ref.partition_id,
                object_id: Some(obj_ref.object_id.clone()),
            };
            let remote = self.remote()?;
            match remote.get_entry(&meta, &key).await {
                Ok(Some(resp)) => Ok(resp.value.map(|v| v.data)),
                Ok(None) => Ok(None),
                Err(e) => Err(convert_proxy_error(e)),
            }
        }
    }

    /// Read multiple fields by key (batch).
    async fn get_many(
        &mut self,
        self_: Resource<ObjectProxyState>,
        keys: Vec<String>,
    ) -> Result<Vec<FieldEntry>, OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        let mut entries = Vec::with_capacity(keys.len());

        if is_local {
            for key in keys {
                match self
                    .data_ops
                    .get_value(
                        &obj_ref.cls,
                        obj_ref.partition_id,
                        &obj_ref.object_id,
                        &key,
                    )
                    .await
                {
                    Ok(Some(data)) => {
                        entries.push(FieldEntry { key, value: data });
                    }
                    Ok(None) => {
                        // Field not found — skip (don't include in result)
                    }
                    Err(e) => return Err(OdgmError::from(e)),
                }
            }
        } else {
            let meta = ObjMeta {
                cls_id: obj_ref.cls.clone(),
                partition_id: obj_ref.partition_id,
                object_id: Some(obj_ref.object_id.clone()),
            };
            let remote = self.remote()?;
            for key in keys {
                match remote.get_entry(&meta, &key).await {
                    Ok(Some(resp)) => {
                        if let Some(val) = resp.value {
                            entries.push(FieldEntry {
                                key,
                                value: val.data,
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(e) => return Err(convert_proxy_error(e)),
                }
            }
        }

        Ok(entries)
    }

    /// Write a single field.
    async fn set(
        &mut self,
        self_: Resource<ObjectProxyState>,
        key: String,
        value: Vec<u8>,
    ) -> Result<(), OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        if is_local {
            self.data_ops
                .set_value(
                    &obj_ref.cls,
                    obj_ref.partition_id,
                    &obj_ref.object_id,
                    &key,
                    value,
                )
                .await
                .map_err(OdgmError::from)
        } else {
            let meta = ObjMeta {
                cls_id: obj_ref.cls.clone(),
                partition_id: obj_ref.partition_id,
                object_id: Some(obj_ref.object_id.clone()),
            };
            let mut values = std::collections::HashMap::new();
            values.insert(
                key,
                oprc_grpc::ValData {
                    data: value,
                    r#type: oprc_grpc::ValType::Byte.into(),
                },
            );
            let req = oprc_grpc::BatchSetValuesRequest {
                cls_id: meta.cls_id.clone(),
                partition_id: meta.partition_id,
                object_id: meta.object_id.clone(),
                values,
                delete_keys: vec![],
                expected_object_version: None,
            };
            let remote = self.remote()?;
            remote
                .batch_set_entries(&req)
                .await
                .map(|_| ())
                .map_err(convert_proxy_error)
        }
    }

    /// Write multiple fields (batch).
    async fn set_many(
        &mut self,
        self_: Resource<ObjectProxyState>,
        field_entries: Vec<FieldEntry>,
    ) -> Result<(), OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        if is_local {
            for entry in field_entries {
                self.data_ops
                    .set_value(
                        &obj_ref.cls,
                        obj_ref.partition_id,
                        &obj_ref.object_id,
                        &entry.key,
                        entry.value,
                    )
                    .await
                    .map_err(OdgmError::from)?;
            }
            Ok(())
        } else {
            let meta = ObjMeta {
                cls_id: obj_ref.cls.clone(),
                partition_id: obj_ref.partition_id,
                object_id: Some(obj_ref.object_id.clone()),
            };
            let values: std::collections::HashMap<String, oprc_grpc::ValData> =
                field_entries
                    .into_iter()
                    .map(|e| {
                        (
                            e.key,
                            oprc_grpc::ValData {
                                data: e.value,
                                r#type: oprc_grpc::ValType::Byte.into(),
                            },
                        )
                    })
                    .collect();
            let req = oprc_grpc::BatchSetValuesRequest {
                cls_id: meta.cls_id.clone(),
                partition_id: meta.partition_id,
                object_id: meta.object_id.clone(),
                values,
                delete_keys: vec![],
                expected_object_version: None,
            };
            let remote = self.remote()?;
            remote
                .batch_set_entries(&req)
                .await
                .map(|_| ())
                .map_err(convert_proxy_error)
        }
    }

    /// Delete a single field.
    async fn delete(
        &mut self,
        self_: Resource<ObjectProxyState>,
        key: String,
    ) -> Result<(), OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        if is_local {
            self.data_ops
                .delete_value(
                    &obj_ref.cls,
                    obj_ref.partition_id,
                    &obj_ref.object_id,
                    &key,
                )
                .await
                .map_err(OdgmError::from)
        } else {
            // Remote delete: there's no dedicated single-value delete via Zenoh proxy,
            // so we set the value to empty bytes which the shard interprets as deletion.
            // A proper delete would need a new Zenoh key expression or proto message.
            // For now, use batch_set_entries with an empty value marker.
            // TODO: Add dedicated remote delete-value via Zenoh
            Err(OdgmError::Internal(
                "remote field delete not yet supported; use set with empty value"
                    .into(),
            ))
        }
    }

    /// Read the full object (all entries).
    async fn get_all(
        &mut self,
        self_: Resource<ObjectProxyState>,
    ) -> Result<ObjData, OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        if is_local {
            match self
                .data_ops
                .get_object(
                    &obj_ref.cls,
                    obj_ref.partition_id,
                    &obj_ref.object_id,
                )
                .await
            {
                Ok(Some(data)) => Ok(bytes_to_obj(data)),
                Ok(None) => Err(OdgmError::NotFound),
                Err(e) => Err(OdgmError::from(e)),
            }
        } else {
            let meta = ObjMeta {
                cls_id: obj_ref.cls.clone(),
                partition_id: obj_ref.partition_id,
                object_id: Some(obj_ref.object_id.clone()),
            };
            let remote = self.remote()?;
            match remote.get_obj(&meta).await {
                Ok(Some(obj)) => {
                    // Convert proto ObjData to WIT ObjData
                    let entries = obj
                        .entries
                        .into_iter()
                        .map(|(k, v)| Entry {
                            key: k,
                            value: ValData {
                                data: v.data,
                                val_type: ValType::Byte,
                            },
                        })
                        .collect();
                    Ok(ObjData {
                        metadata: None,
                        entries,
                    })
                }
                Ok(None) => Err(OdgmError::NotFound),
                Err(e) => Err(convert_proxy_error(e)),
            }
        }
    }

    /// Write the full object (replace all entries).
    async fn set_all(
        &mut self,
        self_: Resource<ObjectProxyState>,
        data: ObjData,
    ) -> Result<(), OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        if is_local {
            self.data_ops
                .set_object(
                    &obj_ref.cls,
                    obj_ref.partition_id,
                    &obj_ref.object_id,
                    obj_to_bytes(&data),
                )
                .await
                .map_err(OdgmError::from)
        } else {
            let proto_obj = oprc_grpc::ObjData {
                metadata: Some(ObjMeta {
                    cls_id: obj_ref.cls.clone(),
                    partition_id: obj_ref.partition_id,
                    object_id: Some(obj_ref.object_id.clone()),
                }),
                event: None,
                entries: data
                    .entries
                    .into_iter()
                    .map(|e| {
                        (
                            e.key,
                            oprc_grpc::ValData {
                                data: e.value.data,
                                r#type: oprc_grpc::ValType::Byte.into(),
                            },
                        )
                    })
                    .collect(),
            };
            let remote = self.remote()?;
            remote
                .set_obj(proto_obj)
                .await
                .map(|_| ())
                .map_err(convert_proxy_error)
        }
    }

    /// Invoke a method on this object.
    async fn invoke(
        &mut self,
        self_: Resource<ObjectProxyState>,
        fn_name: String,
        payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, OdgmError> {
        let proxy = self.get_proxy(&self_)?;
        let obj_ref = proxy.object_ref.clone();
        let is_local = proxy.is_local;

        // Re-entrancy guard
        if self.reentrant_depth >= self.max_reentrant_depth {
            return Err(OdgmError::Internal(format!(
                "max re-entrant depth exceeded ({})",
                self.max_reentrant_depth
            )));
        }
        self.reentrant_depth += 1;

        let result = if is_local {
            self.data_ops
                .invoke_obj(
                    &obj_ref.cls,
                    obj_ref.partition_id,
                    &obj_ref.object_id,
                    &fn_name,
                    payload,
                )
                .await
                .map_err(OdgmError::from)
        } else {
            let meta = ObjMeta {
                cls_id: obj_ref.cls.clone(),
                partition_id: obj_ref.partition_id,
                object_id: Some(obj_ref.object_id.clone()),
            };
            let remote = self.remote()?;
            match remote
                .invoke_object_fn(&meta, &fn_name, payload.unwrap_or_default())
                .await
            {
                Ok(resp) => Ok(resp.payload),
                Err(e) => Err(convert_proxy_error(e)),
            }
        };

        self.reentrant_depth -= 1;
        result
    }

    /// Drop the proxy resource handle.
    async fn drop(
        &mut self,
        rep: Resource<ObjectProxyState>,
    ) -> wasmtime::Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

// ─── Host (context functions) implementation ────────────────

impl object_context::Host for ObjectWasmHostState {
    /// Get a proxy to any accessible object by reference.
    async fn object(
        &mut self,
        ref_: ObjectRef,
    ) -> Result<Resource<ObjectProxyState>, OdgmError> {
        debug!(
            cls = %ref_.cls,
            partition = ref_.partition_id,
            object_id = %ref_.object_id,
            "creating object proxy"
        );
        self.create_proxy(ref_)
    }

    /// Parse "cls/partition/id" string into a proxy.
    async fn object_by_str(
        &mut self,
        ref_str: String,
    ) -> Result<Resource<ObjectProxyState>, OdgmError> {
        let obj_ref = parse_object_ref_str(&ref_str)?;
        self.object(obj_ref).await
    }

    /// Structured logging from guest to host tracing system.
    async fn log(&mut self, level: LogLevel, message: String) {
        match level {
            LogLevel::Debug => debug!(source = "wasm-guest", "{}", message),
            LogLevel::Info => info!(source = "wasm-guest", "{}", message),
            LogLevel::Warn => warn!(source = "wasm-guest", "{}", message),
            LogLevel::Error => error!(source = "wasm-guest", "{}", message),
        }
    }
}

// ─── Utilities ──────────────────────────────────────────────

/// Parse `"cls/partition/id"` into an `ObjectRef`.
///
/// Uses the **first** and **second** `/` as split points.
/// The remainder after the second `/` is the object ID (may not contain `/`).
pub fn parse_object_ref_str(s: &str) -> Result<ObjectRef, OdgmError> {
    let first_slash = s.find('/').ok_or_else(|| {
        OdgmError::InvalidArgument(format!(
            "invalid object ref string '{}': expected 'cls/partition/id'",
            s
        ))
    })?;
    let rest = &s[first_slash + 1..];
    let second_slash = rest.find('/').ok_or_else(|| {
        OdgmError::InvalidArgument(format!(
            "invalid object ref string '{}': expected 'cls/partition/id'",
            s
        ))
    })?;

    let cls = &s[..first_slash];
    let partition_str = &rest[..second_slash];
    let object_id = &rest[second_slash + 1..];

    if cls.is_empty() {
        return Err(OdgmError::InvalidArgument(
            "class name must not be empty".into(),
        ));
    }
    if object_id.is_empty() {
        return Err(OdgmError::InvalidArgument(
            "object_id must not be empty".into(),
        ));
    }

    let partition_id: u32 = partition_str.parse().map_err(|_| {
        OdgmError::InvalidArgument(format!(
            "invalid partition id '{}': must be a u32",
            partition_str
        ))
    })?;

    Ok(ObjectRef {
        cls: cls.to_string(),
        partition_id,
        object_id: object_id.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock_ops::MockDataOps;
    use crate::oaas_object_world::oaas::odgm::object_context::{
        Host, HostObjectProxy,
    };

    /// Helper: build an ObjectWasmHostState backed by MockDataOps.
    fn make_state(cls: &str, partition: u32) -> ObjectWasmHostState {
        let mock = MockDataOps::default();
        let ctx = wasmtime_wasi::p2::WasiCtxBuilder::new().build();
        ObjectWasmHostState::new(
            Box::new(mock),
            None,
            cls.to_string(),
            partition,
            ctx,
        )
    }

    /// Helper: build state and return both state + mock for inspection.
    fn make_state_with_mock(
        cls: &str,
        partition: u32,
    ) -> (ObjectWasmHostState, MockDataOps) {
        let mock = MockDataOps::default();
        let mock_clone = mock.clone();
        let ctx = wasmtime_wasi::p2::WasiCtxBuilder::new().build();
        let state = ObjectWasmHostState::new(
            Box::new(mock),
            None,
            cls.to_string(),
            partition,
            ctx,
        );
        (state, mock_clone)
    }

    // ─── parse_object_ref_str ───────────────────────────────

    #[test]
    fn parse_valid_ref() {
        let r = parse_object_ref_str("MyClass/42/obj-123").unwrap();
        assert_eq!(r.cls, "MyClass");
        assert_eq!(r.partition_id, 42);
        assert_eq!(r.object_id, "obj-123");
    }

    #[test]
    fn parse_zero_partition() {
        let r = parse_object_ref_str("svc/0/abc").unwrap();
        assert_eq!(r.partition_id, 0);
    }

    #[test]
    fn parse_missing_slash() {
        assert!(parse_object_ref_str("noslash").is_err());
    }

    #[test]
    fn parse_missing_second_slash() {
        assert!(parse_object_ref_str("cls/42").is_err());
    }

    #[test]
    fn parse_empty_cls() {
        assert!(parse_object_ref_str("/42/obj").is_err());
    }

    #[test]
    fn parse_empty_object_id() {
        assert!(parse_object_ref_str("cls/42/").is_err());
    }

    #[test]
    fn parse_invalid_partition() {
        assert!(parse_object_ref_str("cls/abc/obj").is_err());
    }

    #[test]
    fn parse_large_partition() {
        let r = parse_object_ref_str("cls/4294967295/obj").unwrap();
        assert_eq!(r.partition_id, u32::MAX);
    }

    #[test]
    fn parse_partition_overflow() {
        assert!(parse_object_ref_str("cls/4294967296/obj").is_err());
    }

    #[test]
    fn parse_hyphenated_class_and_id() {
        let r = parse_object_ref_str("my-class/1/my-obj-id").unwrap();
        assert_eq!(r.cls, "my-class");
        assert_eq!(r.object_id, "my-obj-id");
    }

    // ─── ObjectWasmHostState: locality checks ───────────────

    #[test]
    fn is_local_same_shard() {
        let state = make_state("myclass", 5);
        let local_ref = ObjectRef {
            cls: "myclass".to_string(),
            partition_id: 5,
            object_id: "x".to_string(),
        };
        assert!(state.is_local(&local_ref));
    }

    #[test]
    fn is_local_different_class() {
        let state = make_state("myclass", 5);
        let remote_ref = ObjectRef {
            cls: "other".to_string(),
            partition_id: 5,
            object_id: "x".to_string(),
        };
        assert!(!state.is_local(&remote_ref));
    }

    #[test]
    fn is_local_different_partition() {
        let state = make_state("myclass", 5);
        let remote_ref = ObjectRef {
            cls: "myclass".to_string(),
            partition_id: 99,
            object_id: "x".to_string(),
        };
        assert!(!state.is_local(&remote_ref));
    }

    // ─── Proxy lifecycle ────────────────────────────────────

    #[test]
    fn create_and_get_proxy() {
        let mut state = make_state("cls", 0);
        let obj_ref = ObjectRef {
            cls: "cls".to_string(),
            partition_id: 0,
            object_id: "o1".to_string(),
        };
        let handle = state.create_proxy(obj_ref.clone()).unwrap();
        let proxy = state.get_proxy(&handle).unwrap();
        assert_eq!(proxy.object_ref.cls, "cls");
        assert_eq!(proxy.object_ref.object_id, "o1");
        assert!(proxy.is_local);
    }

    #[test]
    fn create_remote_proxy() {
        let mut state = make_state("cls-a", 0);
        let remote_ref = ObjectRef {
            cls: "cls-b".to_string(),
            partition_id: 7,
            object_id: "remote-obj".to_string(),
        };
        let handle = state.create_proxy(remote_ref).unwrap();
        let proxy = state.get_proxy(&handle).unwrap();
        assert!(!proxy.is_local);
        assert_eq!(proxy.object_ref.cls, "cls-b");
    }

    #[test]
    fn multiple_proxies_are_independent() {
        let mut state = make_state("cls", 0);
        let h1 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "a".to_string(),
            })
            .unwrap();
        let h2 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "b".to_string(),
            })
            .unwrap();

        assert_eq!(state.get_proxy(&h1).unwrap().object_ref.object_id, "a");
        assert_eq!(state.get_proxy(&h2).unwrap().object_ref.object_id, "b");
    }

    // ─── Host trait: object / object_by_str ─────────────────

    #[tokio::test]
    async fn host_object_creates_proxy() {
        let mut state = make_state("svc", 3);
        let obj_ref = ObjectRef {
            cls: "svc".to_string(),
            partition_id: 3,
            object_id: "item-1".to_string(),
        };
        let handle = Host::object(&mut state, obj_ref).await.unwrap();
        let proxy = state.get_proxy(&handle).unwrap();
        assert_eq!(proxy.object_ref.object_id, "item-1");
        assert!(proxy.is_local);
    }

    #[tokio::test]
    async fn host_object_by_str_valid() {
        let mut state = make_state("svc", 3);
        let handle =
            Host::object_by_str(&mut state, "svc/3/item-2".to_string())
                .await
                .unwrap();
        let proxy = state.get_proxy(&handle).unwrap();
        assert_eq!(proxy.object_ref.cls, "svc");
        assert_eq!(proxy.object_ref.partition_id, 3);
        assert_eq!(proxy.object_ref.object_id, "item-2");
    }

    #[tokio::test]
    async fn host_object_by_str_invalid() {
        let mut state = make_state("svc", 3);
        let result =
            Host::object_by_str(&mut state, "bad-ref".to_string()).await;
        assert!(result.is_err());
    }

    // ─── Host trait: log ────────────────────────────────────

    #[tokio::test]
    async fn host_log_does_not_panic() {
        let mut state = make_state("svc", 0);
        Host::log(&mut state, LogLevel::Debug, "debug msg".into()).await;
        Host::log(&mut state, LogLevel::Info, "info msg".into()).await;
        Host::log(&mut state, LogLevel::Warn, "warn msg".into()).await;
        Host::log(&mut state, LogLevel::Error, "error msg".into()).await;
        // Just verify no panic — tracing output is not asserted here.
    }

    // ─── HostObjectProxy: ref_ ──────────────────────────────

    #[tokio::test]
    async fn proxy_ref_returns_identity() {
        let mut state = make_state("cls", 0);
        let handle = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-42".to_string(),
            })
            .unwrap();
        let r = HostObjectProxy::ref_(&mut state, handle).await;
        assert_eq!(r.cls, "cls");
        assert_eq!(r.object_id, "obj-42");
    }

    // ─── HostObjectProxy: local get/set/delete (granular) ───

    #[tokio::test]
    async fn proxy_set_and_get_local() {
        let (mut state, _mock) = make_state_with_mock("cls", 0);
        let handle = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-1".to_string(),
            })
            .unwrap();

        // set
        let h2 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-1".to_string(),
            })
            .unwrap();
        HostObjectProxy::set(
            &mut state,
            handle,
            "field-a".into(),
            b"value-a".to_vec(),
        )
        .await
        .unwrap();

        // get
        let val = HostObjectProxy::get(&mut state, h2, "field-a".into())
            .await
            .unwrap();
        assert_eq!(val, Some(b"value-a".to_vec()));
    }

    #[tokio::test]
    async fn proxy_get_missing_key_returns_none() {
        let (mut state, _) = make_state_with_mock("cls", 0);
        let handle = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-1".to_string(),
            })
            .unwrap();
        let val =
            HostObjectProxy::get(&mut state, handle, "nonexistent".into())
                .await
                .unwrap();
        assert_eq!(val, None);
    }

    #[tokio::test]
    async fn proxy_delete_local() {
        let (mut state, _) = make_state_with_mock("cls", 0);
        let h1 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-1".to_string(),
            })
            .unwrap();
        let h2 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-1".to_string(),
            })
            .unwrap();
        let h3 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-1".to_string(),
            })
            .unwrap();

        HostObjectProxy::set(&mut state, h1, "k".into(), b"v".to_vec())
            .await
            .unwrap();
        HostObjectProxy::delete(&mut state, h2, "k".into())
            .await
            .unwrap();
        let val = HostObjectProxy::get(&mut state, h3, "k".into())
            .await
            .unwrap();
        assert_eq!(val, None);
    }

    // ─── HostObjectProxy: get_many / set_many ───────────────

    #[tokio::test]
    async fn proxy_set_many_and_get_many() {
        let (mut state, _) = make_state_with_mock("cls", 0);
        let h1 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-batch".to_string(),
            })
            .unwrap();
        let h2 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-batch".to_string(),
            })
            .unwrap();

        let entries = vec![
            FieldEntry {
                key: "x".into(),
                value: b"1".to_vec(),
            },
            FieldEntry {
                key: "y".into(),
                value: b"2".to_vec(),
            },
            FieldEntry {
                key: "z".into(),
                value: b"3".to_vec(),
            },
        ];
        HostObjectProxy::set_many(&mut state, h1, entries)
            .await
            .unwrap();

        let result = HostObjectProxy::get_many(
            &mut state,
            h2,
            vec!["x".into(), "z".into(), "missing".into()],
        )
        .await
        .unwrap();

        // "missing" should not appear in results
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|e| e.key == "x" && e.value == b"1"));
        assert!(result.iter().any(|e| e.key == "z" && e.value == b"3"));
    }

    // ─── HostObjectProxy: get_all / set_all ─────────────────

    #[tokio::test]
    async fn proxy_set_all_and_get_all() {
        let (mut state, _) = make_state_with_mock("cls", 0);
        let h1 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-all".to_string(),
            })
            .unwrap();
        let h2 = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj-all".to_string(),
            })
            .unwrap();

        let obj = ObjData {
            metadata: None,
            entries: vec![Entry {
                key: "_raw".into(),
                value: ValData {
                    data: b"full-object-data".to_vec(),
                    val_type: ValType::Byte,
                },
            }],
        };
        HostObjectProxy::set_all(&mut state, h1, obj).await.unwrap();

        let result = HostObjectProxy::get_all(&mut state, h2).await.unwrap();
        assert!(!result.entries.is_empty());
        assert_eq!(result.entries[0].value.data, b"full-object-data");
    }

    #[tokio::test]
    async fn proxy_get_all_missing_returns_not_found() {
        let (mut state, _) = make_state_with_mock("cls", 0);
        let handle = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "nonexistent".to_string(),
            })
            .unwrap();
        let result = HostObjectProxy::get_all(&mut state, handle).await;
        assert!(matches!(result, Err(OdgmError::NotFound)));
    }

    // ─── HostObjectProxy: invoke (re-entrancy guard) ────────

    #[tokio::test]
    async fn proxy_invoke_increments_and_decrements_depth() {
        let (mut state, _) = make_state_with_mock("cls", 0);
        assert_eq!(state.reentrant_depth, 0);

        let handle = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj".to_string(),
            })
            .unwrap();

        // invoke_obj is not implemented in mock, so it will error but depth
        // should still be decremented on error path
        let _result =
            HostObjectProxy::invoke(&mut state, handle, "some-fn".into(), None)
                .await;

        // Depth should be back to 0 after the call (even on error)
        assert_eq!(state.reentrant_depth, 0);
    }

    #[tokio::test]
    async fn proxy_invoke_rejects_at_max_depth() {
        let (mut state, _) = make_state_with_mock("cls", 0);
        state.reentrant_depth = state.max_reentrant_depth; // saturate

        let handle = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj".to_string(),
            })
            .unwrap();

        let result =
            HostObjectProxy::invoke(&mut state, handle, "fn".into(), None)
                .await;

        assert!(matches!(result, Err(OdgmError::Internal(_))));
        // depth should NOT have been incremented
        assert_eq!(state.reentrant_depth, state.max_reentrant_depth);
    }

    // ─── HostObjectProxy: remote ops without proxy ──────────

    #[tokio::test]
    async fn remote_get_fails_without_zenoh_proxy() {
        let (mut state, _) = make_state_with_mock("local-cls", 0);
        // Create a proxy to a *different* class → remote
        let handle = state
            .create_proxy(ObjectRef {
                cls: "remote-cls".to_string(),
                partition_id: 0,
                object_id: "obj".to_string(),
            })
            .unwrap();

        let result =
            HostObjectProxy::get(&mut state, handle, "key".into()).await;
        assert!(matches!(result, Err(OdgmError::Internal(_))));
    }

    #[tokio::test]
    async fn remote_set_fails_without_zenoh_proxy() {
        let (mut state, _) = make_state_with_mock("local-cls", 0);
        let handle = state
            .create_proxy(ObjectRef {
                cls: "remote-cls".to_string(),
                partition_id: 0,
                object_id: "obj".to_string(),
            })
            .unwrap();

        let result = HostObjectProxy::set(
            &mut state,
            handle,
            "key".into(),
            b"val".to_vec(),
        )
        .await;
        assert!(matches!(result, Err(OdgmError::Internal(_))));
    }

    #[tokio::test]
    async fn remote_invoke_fails_without_zenoh_proxy() {
        let (mut state, _) = make_state_with_mock("local-cls", 0);
        let handle = state
            .create_proxy(ObjectRef {
                cls: "remote-cls".to_string(),
                partition_id: 0,
                object_id: "obj".to_string(),
            })
            .unwrap();

        let result =
            HostObjectProxy::invoke(&mut state, handle, "fn".into(), None)
                .await;
        assert!(matches!(result, Err(OdgmError::Internal(_))));
    }

    // ─── HostObjectProxy: drop ──────────────────────────────

    #[tokio::test]
    async fn proxy_drop_removes_from_table() {
        let mut state = make_state("cls", 0);
        let handle = state
            .create_proxy(ObjectRef {
                cls: "cls".to_string(),
                partition_id: 0,
                object_id: "obj".to_string(),
            })
            .unwrap();

        // Before drop — should be accessible
        assert!(state.get_proxy(&handle).is_ok());

        // Drop it
        HostObjectProxy::drop(&mut state, handle).await.unwrap();

        // After drop — creating a new handle at the same rep should work,
        // but the old handle resource is gone from the table
    }

    // ─── ObjectProxyState ───────────────────────────────────

    #[test]
    fn proxy_state_to_obj_meta() {
        let state = ObjectProxyState::new(
            ObjectRef {
                cls: "svc".to_string(),
                partition_id: 7,
                object_id: "abc".to_string(),
            },
            true,
        );
        let meta = state.to_obj_meta();
        assert_eq!(meta.cls_id, "svc");
        assert_eq!(meta.partition_id, 7);
        assert_eq!(meta.object_id, Some("abc".to_string()));
    }
}
