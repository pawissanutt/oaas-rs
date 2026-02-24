use crate::host::{DataOpsError, OdgmDataOps};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Default, Clone)]
pub struct MockDataOps {
    // collection -> object_id -> data
    objects: Arc<RwLock<HashMap<String, HashMap<String, Vec<u8>>>>>,
}

#[async_trait]
impl OdgmDataOps for MockDataOps {
    async fn get_object(
        &self,
        cls_id: &str,
        _part: u32,
        obj_id: &str,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        let guard = self.objects.read().await;
        if let Some(col) = guard.get(cls_id) {
            Ok(col.get(obj_id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn set_object(
        &self,
        cls_id: &str,
        _part: u32,
        obj_id: &str,
        data: Vec<u8>,
    ) -> Result<(), DataOpsError> {
        let mut guard = self.objects.write().await;
        let col = guard.entry(cls_id.to_string()).or_default();
        col.insert(obj_id.to_string(), data);
        Ok(())
    }

    async fn delete_object(
        &self,
        cls_id: &str,
        _part: u32,
        obj_id: &str,
    ) -> Result<(), DataOpsError> {
        let mut guard = self.objects.write().await;
        if let Some(col) = guard.get_mut(cls_id) {
            col.remove(obj_id);
        }
        Ok(())
    }

    async fn get_value(
        &self,
        _cls_id: &str,
        _part: u32,
        _obj_id: &str,
        _key: &str,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        Err(DataOpsError::Internal("Not implemented in mock".into()))
    }

    async fn set_value(
        &self,
        _cls_id: &str,
        _part: u32,
        _obj_id: &str,
        _key: &str,
        _val: Vec<u8>,
    ) -> Result<(), DataOpsError> {
        Err(DataOpsError::Internal("Not implemented in mock".into()))
    }

    async fn delete_value(
        &self,
        _cls_id: &str,
        _part: u32,
        _obj_id: &str,
        _key: &str,
    ) -> Result<(), DataOpsError> {
        Err(DataOpsError::Internal("Not implemented in mock".into()))
    }

    async fn invoke_fn(
        &self,
        _cls_id: &str,
        _part: u32,
        _fn_id: &str,
        _payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        Err(DataOpsError::Internal("Not implemented in mock".into()))
    }
}
