use std::sync::Arc;

use tokio::sync::Mutex;

use crate::OprcZenohConfig;

struct PoolInner(Vec<zenoh::Session>, usize);

#[derive(Clone)]
pub struct Pool {
    inner: Arc<Mutex<PoolInner>>,
    max_sessions: usize,
    z_conf: OprcZenohConfig,
}

impl Pool {
    pub fn new(max_sessions: usize, z_conf: OprcZenohConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PoolInner(vec![], 0))),
            max_sessions,
            z_conf,
        }
    }

    pub async fn get_session(
        &self,
    ) -> Result<zenoh::Session, Box<dyn std::error::Error + Send + Sync>> {
        let mut pool = self.inner.lock().await;
        pool.1 += 1;
        if pool.0.len() < self.max_sessions as usize {
            let session = zenoh::open(self.z_conf.create_zenoh()).await?;
            pool.0.push(session);
        }
        Ok(pool.0[pool.1 % pool.0.len()].clone())
    }
}
