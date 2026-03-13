use crate::host::{DataOpsError, OdgmDataOps};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Default, Clone)]
pub struct MockDataOps {
    // collection -> object_id -> data  (whole-object storage)
    objects: Arc<RwLock<HashMap<String, HashMap<String, Vec<u8>>>>>,
    // (collection, object_id) -> (key -> data)  (granular per-field storage)
    entries: Arc<RwLock<HashMap<(String, String), HashMap<String, Vec<u8>>>>>,
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
        // Also remove all granular entries
        let mut entries = self.entries.write().await;
        entries.remove(&(cls_id.to_string(), obj_id.to_string()));
        Ok(())
    }

    async fn get_value(
        &self,
        cls_id: &str,
        _part: u32,
        obj_id: &str,
        key: &str,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        let guard = self.entries.read().await;
        let k = (cls_id.to_string(), obj_id.to_string());
        if let Some(fields) = guard.get(&k) {
            Ok(fields.get(key).cloned())
        } else {
            Ok(None)
        }
    }

    async fn set_value(
        &self,
        cls_id: &str,
        _part: u32,
        obj_id: &str,
        key: &str,
        val: Vec<u8>,
    ) -> Result<(), DataOpsError> {
        let mut guard = self.entries.write().await;
        let k = (cls_id.to_string(), obj_id.to_string());
        let fields = guard.entry(k).or_default();
        fields.insert(key.to_string(), val);
        Ok(())
    }

    async fn delete_value(
        &self,
        cls_id: &str,
        _part: u32,
        obj_id: &str,
        key: &str,
    ) -> Result<(), DataOpsError> {
        let mut guard = self.entries.write().await;
        let k = (cls_id.to_string(), obj_id.to_string());
        if let Some(fields) = guard.get_mut(&k) {
            fields.remove(key);
        }
        Ok(())
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

    async fn invoke_obj(
        &self,
        _cls_id: &str,
        _part: u32,
        _object_id: &str,
        _fn_id: &str,
        _payload: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, DataOpsError> {
        Err(DataOpsError::Internal("Not implemented in mock".into()))
    }

    async fn get_all_entries(
        &self,
        cls_id: &str,
        _part: u32,
        obj_id: &str,
    ) -> Result<Option<Vec<(String, Vec<u8>)>>, DataOpsError> {
        let guard = self.entries.read().await;
        let k = (cls_id.to_string(), obj_id.to_string());
        match guard.get(&k) {
            Some(fields) => Ok(Some(
                fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            )),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn object_crud() {
        let mock = MockDataOps::default();
        // Initially empty
        assert!(mock.get_object("c", 0, "o").await.unwrap().is_none());

        // Set and get
        mock.set_object("c", 0, "o", b"data".to_vec())
            .await
            .unwrap();
        assert_eq!(
            mock.get_object("c", 0, "o").await.unwrap(),
            Some(b"data".to_vec())
        );

        // Delete
        mock.delete_object("c", 0, "o").await.unwrap();
        assert!(mock.get_object("c", 0, "o").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn granular_entry_crud() {
        let mock = MockDataOps::default();

        // Initially empty
        assert!(mock.get_value("c", 0, "o", "k").await.unwrap().is_none());

        // Set and get
        mock.set_value("c", 0, "o", "k", b"v".to_vec())
            .await
            .unwrap();
        assert_eq!(
            mock.get_value("c", 0, "o", "k").await.unwrap(),
            Some(b"v".to_vec())
        );

        // Delete
        mock.delete_value("c", 0, "o", "k").await.unwrap();
        assert!(mock.get_value("c", 0, "o", "k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn entries_independent_per_object() {
        let mock = MockDataOps::default();
        mock.set_value("c", 0, "o1", "k", b"v1".to_vec())
            .await
            .unwrap();
        mock.set_value("c", 0, "o2", "k", b"v2".to_vec())
            .await
            .unwrap();

        assert_eq!(
            mock.get_value("c", 0, "o1", "k").await.unwrap(),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            mock.get_value("c", 0, "o2", "k").await.unwrap(),
            Some(b"v2".to_vec())
        );
    }

    #[tokio::test]
    async fn multiple_keys_per_object() {
        let mock = MockDataOps::default();
        mock.set_value("c", 0, "o", "a", b"1".to_vec())
            .await
            .unwrap();
        mock.set_value("c", 0, "o", "b", b"2".to_vec())
            .await
            .unwrap();
        mock.set_value("c", 0, "o", "c_key", b"3".to_vec())
            .await
            .unwrap();

        assert_eq!(
            mock.get_value("c", 0, "o", "a").await.unwrap(),
            Some(b"1".to_vec())
        );
        assert_eq!(
            mock.get_value("c", 0, "o", "b").await.unwrap(),
            Some(b"2".to_vec())
        );
        assert_eq!(
            mock.get_value("c", 0, "o", "c_key").await.unwrap(),
            Some(b"3".to_vec())
        );
    }

    #[tokio::test]
    async fn delete_object_clears_entries() {
        let mock = MockDataOps::default();
        mock.set_value("c", 0, "o", "k", b"v".to_vec())
            .await
            .unwrap();
        mock.set_object("c", 0, "o", b"obj-data".to_vec())
            .await
            .unwrap();

        // Delete the object
        mock.delete_object("c", 0, "o").await.unwrap();

        // Both object-level and entry-level data should be gone
        assert!(mock.get_object("c", 0, "o").await.unwrap().is_none());
        assert!(mock.get_value("c", 0, "o", "k").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn overwrite_entry_value() {
        let mock = MockDataOps::default();
        mock.set_value("c", 0, "o", "k", b"first".to_vec())
            .await
            .unwrap();
        mock.set_value("c", 0, "o", "k", b"second".to_vec())
            .await
            .unwrap();
        assert_eq!(
            mock.get_value("c", 0, "o", "k").await.unwrap(),
            Some(b"second".to_vec())
        );
    }

    #[tokio::test]
    async fn invoke_fn_returns_error() {
        let mock = MockDataOps::default();
        let result = mock.invoke_fn("c", 0, "fn", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn invoke_obj_returns_error() {
        let mock = MockDataOps::default();
        let result = mock.invoke_obj("c", 0, "o", "fn", None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let mock = MockDataOps::default();
        let clone = mock.clone();

        mock.set_value("c", 0, "o", "k", b"v".to_vec())
            .await
            .unwrap();
        assert_eq!(
            clone.get_value("c", 0, "o", "k").await.unwrap(),
            Some(b"v".to_vec())
        );
    }
}
