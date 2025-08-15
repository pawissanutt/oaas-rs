use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::crd::deployment_record::DeploymentRecord;

#[derive(Clone)]
pub struct DeploymentRecordCache(
    pub Arc<RwLock<HashMap<String, DeploymentRecord>>>,
);

impl DeploymentRecordCache {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashMap::new())))
    }

    pub async fn upsert(&self, key: String, dr: DeploymentRecord) {
        let mut w = self.0.write().await;
        w.insert(key, dr);
    }

    pub async fn remove(&self, key: &str) {
        let mut w = self.0.write().await;
        w.remove(key);
    }

    pub async fn list(&self) -> Vec<DeploymentRecord> {
        let r = self.0.read().await;
        r.values().cloned().collect()
    }
}
