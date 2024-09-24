use std::{fmt::Debug, hash::Hash, sync::Arc, time::Duration};

use hashbrown::HashMap;
use mobc::{Builder, Connection, Manager, Pool, State};
use tokio::sync::RwLock;
use tracing::info;

#[async_trait::async_trait]
pub trait ConnFactory<K, T>: Send + Sync
where
    K: Eq + Hash,
    T: Manager,
    T::Error: std::error::Error,
{
    async fn create(&self, key: K) -> Result<T, T::Error>;
}

#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_open: u64,
    pub max_idle: u64,
    pub max_lifetime: Option<Duration>,
    pub max_idle_lifetime: Option<Duration>,
    pub get_timeout: Option<Duration>,
    pub health_check_interval: Option<Duration>,
    pub health_check: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_open: 32,
            max_idle: 0,
            max_lifetime: None,
            max_idle_lifetime: None,
            get_timeout: Some(Duration::from_secs(30)),
            health_check: true,
            health_check_interval: None,
        }
    }
}

impl PoolConfig {
    pub fn get_builder<M: Manager>(&self) -> Builder<M> {
        Builder::new()
            .max_open(self.max_open)
            .max_idle(self.max_idle)
            .max_lifetime(self.max_lifetime)
            .max_idle_lifetime(self.max_idle_lifetime)
            .health_check_interval(self.health_check_interval)
            .test_on_check_out(self.health_check)
            .get_timeout(self.get_timeout)
    }
}

pub struct ConnManager<K, T>
where
    K: Eq + Hash,
    T: Manager,
    T::Error: std::error::Error,
{
    pool_map: RwLock<HashMap<K, Arc<Pool<T>>>>,
    factory: Arc<dyn ConnFactory<K, T>>,
    conf: PoolConfig,
}

impl<K, T> ConnManager<K, T>
where
    K: Eq + Hash + Clone + Debug,
    T: Manager,
    T::Error: std::error::Error,
{
    pub fn new(factory: Arc<dyn ConnFactory<K, T>>) -> Self {
        Self {
            pool_map: RwLock::new(HashMap::new()),
            factory,
            conf: PoolConfig::default(),
        }
    }

    pub fn new_with_builder(
        factory: Arc<dyn ConnFactory<K, T>>,
        conf: PoolConfig,
    ) -> Self {
        Self {
            pool_map: RwLock::new(HashMap::new()),
            factory,
            conf,
        }
    }

    pub async fn get(
        &self,
        key: K,
    ) -> Result<Connection<T>, mobc::Error<T::Error>> {
        let pool_map = self.pool_map.read().await;
        if let Some(pool) = pool_map.get(&key) {
            pool.get().await
        } else {
            info!("create pool for '{:?}'", key);
            drop(pool_map);
            let mut pool_map = self.pool_map.write().await;
            let manager = self.factory.create(key.clone()).await?;
            let pool = self.conf.get_builder().build(manager);
            pool_map.insert(key.clone(), Arc::new(pool));
            let pool = pool_map.get(&key).unwrap();
            pool.get().await
        }
    }

    pub async fn get_states(&self) -> HashMap<K, State> {
        let pool_map = self.pool_map.read().await;
        let mut map = HashMap::new();
        for (k, v) in pool_map.iter() {
            map.insert(k.clone(), v.state().await);
        }
        map
    }
}
