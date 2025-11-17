use std::sync::Arc;
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
        // Optional global sharing: when OPRC_ZENOH_SHARE_SESSION is set ("1"/"true"),
        // all Pool instances in the same process will use the same underlying PoolInner,
        // effectively sharing a single zenoh::Session across logical nodes in tests.
        static SHARED_INNER: OnceLock<Arc<Mutex<PoolInner>>> = OnceLock::new();
        let share = std::env::var("OPRC_ZENOH_SHARE_SESSION")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);

        // Strategy:
        // 1. If user provided a port (env OPRC_ZENOH_PORT), honor it and allocate subsequent
        //    session listen ports sequentially from this base (base + index).
        // 2. Else, allow OS to pick an ephemeral port by using port 0 (avoids cross-process collisions).
        //    In this mode we don't pre-compute listen ports and rely on a single session by default
        //    (max_sessions is usually 1 for ODGM). Tests that need multiple connected sessions can
        //    still provide an explicit base port via env/config.
        let base_port = z_conf.zenoh_port; // 0 means "let OS choose"
        let inner = if share {
            SHARED_INNER
                .get_or_init(|| {
                    Arc::new(Mutex::new(PoolInner {
                        sessions: vec![],
                        next_idx: 0,
                        base_port,
                    }))
                })
                .clone()
        } else {
            Arc::new(Mutex::new(PoolInner {
                sessions: vec![],
                next_idx: 0,
                base_port,
            }))
        };
        Self {
            inner,
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
            // Assign a unique listen port per session deterministically only when a base port was provided.
            // Otherwise, keep port 0 to let the OS choose a free ephemeral port (collision-free across processes).
            if pool.base_port != 0 {
                let listen_port =
                    pool.base_port.saturating_add(pool.sessions.len() as u16);
                z_conf.zenoh_port = listen_port;
            } else {
                // Ensure we keep 0 so zenoh binds to an ephemeral free port
                z_conf.zenoh_port = 0;
                // Enable zenoh gossip discovery when using ephemeral ports so sessions can auto-discover peers.
                if z_conf.gossip_enabled.is_none() {
                    z_conf.gossip_enabled = Some(true);
                }
            }
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
                // Only register when we know the concrete listen port (non-zero base path)
                if pool.base_port != 0 {
                    let listen_port = pool
                        .base_port
                        .saturating_add((pool.sessions.len() - 1) as u16);
                    if !ports.contains(&listen_port) {
                        ports.push(listen_port);
                    }
                }
            }
        }
        Ok(pool.sessions[pool.next_idx % pool.sessions.len()].clone())
    }

    /// Gracefully close all Zenoh sessions managed by this pool to ensure background tasks terminate.
    pub async fn close(&self) {
        // If sessions are shared globally (test convenience), avoid explicitly closing them here.
        // Let them be dropped at process end or when the shared Arc is dropped.
        let share = std::env::var("OPRC_ZENOH_SHARE_SESSION")
            .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true"))
            .unwrap_or(false);
        if share {
            tracing::debug!(
                "zenoh pool close() skipped due to OPRC_ZENOH_SHARE_SESSION"
            );
            return;
        }

        let mut pool = self.inner.lock().await;
        for session in pool.sessions.drain(..) {
            if let Err(e) = session.close().await {
                tracing::warn!("zenoh pool session close error: {e}");
            }
        }
        pool.next_idx = 0;
    }
}
