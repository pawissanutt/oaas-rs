use std::hash::Hash;

use hashbrown::HashMap;
use mobc::{Connection, Manager, Pool};
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct ConnManager<K, T>
where
    K: Eq + Hash,
    T: Manager,
    T::Error: std::error::Error,
{
    pool_map: RwLock<HashMap<K, Pool<T>>>,
    factory: fn(K) -> Result<T, T::Error>,
}

impl<K, T> ConnManager<K, T>
where
    K: Eq + Hash + Clone,
    T: Manager,
    T::Error: std::error::Error,
{
    pub fn new(factory: fn(K) -> Result<T, T::Error>) -> Self {
        Self {
            pool_map: RwLock::new(HashMap::new()),
            factory,
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
            drop(pool_map);
            let mut pool_map = self.pool_map.write().await;
            let manager = (self.factory)(key.clone())?;
            let pool = mobc::Pool::builder().build(manager);
            pool_map.insert(key.clone(), pool);
            let pool = pool_map.get(&key).unwrap();
            pool.get().await
        }
    }
}
