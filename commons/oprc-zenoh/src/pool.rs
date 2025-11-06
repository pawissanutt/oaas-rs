use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock};

use tokio::sync::Mutex;

use crate::OprcZenohConfig;

struct PoolInner {
    sessions: Vec<zenoh::Session>,
    next_idx: usize,
    base_port: u16,
}

#[derive(Clone)]
pub struct Pool {
    inner: Arc<Mutex<PoolInner>>,
    max_sessions: usize,
    z_conf: OprcZenohConfig,
}

impl Pool {
    pub fn new(max_sessions: usize, z_conf: OprcZenohConfig) -> Self {
        // If no explicit base port provided, pick a deterministic high port range suitable for tests,
        // and ensure different pools in the same process don't collide by reserving ranges of size 256.
        static NEXT_BASE: OnceLock<AtomicU16> = OnceLock::new();
        let counter = NEXT_BASE.get_or_init(|| AtomicU16::new(45000));
        let base_port = if z_conf.zenoh_port == 0 {
            // Reserve a block of 256 ports per pool
            counter.fetch_add(256, Ordering::SeqCst)
        } else {
            z_conf.zenoh_port
        };
        Self {
            inner: Arc::new(Mutex::new(PoolInner {
                sessions: vec![],
                next_idx: 0,
                base_port,
            })),
            max_sessions,
            z_conf,
        }
    }

    pub async fn get_session(
        &self,
    ) -> Result<zenoh::Session, Box<dyn std::error::Error + Send + Sync>> {
        let mut pool = self.inner.lock().await;
        pool.next_idx += 1;
        if pool.sessions.len() < self.max_sessions as usize {
            let mut z_conf = self.z_conf.clone();
            // Assign a unique listen port per session deterministically
            let listen_port =
                pool.base_port.saturating_add(pool.sessions.len() as u16);
            z_conf.zenoh_port = listen_port;
            // Connect to all previously registered sessions across all pools via localhost
            static GLOBAL_PORTS: OnceLock<StdMutex<Vec<u16>>> = OnceLock::new();
            let ports_mutex =
                GLOBAL_PORTS.get_or_init(|| StdMutex::new(Vec::new()));
            let ports_snapshot = {
                let ports = ports_mutex.lock().unwrap();
                ports.clone()
            };
            if !ports_snapshot.is_empty() {
                let peers: Vec<String> = ports_snapshot
                    .into_iter()
                    .map(|p| format!("tcp/127.0.0.1:{}", p))
                    .collect();
                z_conf.peers = Some(peers.join(","));
            }
            let session = zenoh::open(z_conf.create_zenoh()).await?;
            pool.sessions.push(session);
            // Register our listen port globally for future sessions in this process
            {
                let mut ports = ports_mutex.lock().unwrap();
                if !ports.contains(&listen_port) {
                    ports.push(listen_port);
                }
            }
        }
        Ok(pool.sessions[pool.next_idx % pool.sessions.len()].clone())
    }
}
