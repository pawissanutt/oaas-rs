//! ObjectUnifiedShard - Unified shard combining storage, networking, events, and management
//!
//! This module is split into:
//! - `mod.rs` - struct definition and re-exports
//! - `core.rs` - core implementation (internal methods, constructors)
//! - `trait_impl.rs` - ObjectShard trait implementation
//! - `transaction.rs` - transaction adapter

mod core;
mod trait_impl;
mod transaction;

use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell, watch};
use tokio_util::sync::CancellationToken;

use super::network::UnifiedShardNetwork;
use crate::shard::{ShardMetadata, ShardMetrics, ShardOptions};
use crate::events::EventManager;
use crate::replication::ReplicationLayer;
use crate::shard::invocation::{InvocationNetworkManager, InvocationOffloader};
use crate::shard::liveliness::MemberLivelinessState;
use oprc_dp_storage::ApplicationDataStorage;

pub use transaction::UnifiedShardWriteTxAdapter;

/// Unified ObjectShard that combines storage, networking, events, and management
pub struct ObjectUnifiedShard<A, R, E>
where
    A: ApplicationDataStorage,
    R: ReplicationLayer,
    E: EventManager + Send + Sync + 'static,
{
    // Core storage and metadata (merged from UnifiedShard)
    pub(crate) metadata: ShardMetadata,
    pub(crate) app_storage: A, // Direct access to application storage - log storage is now part of replication
    pub(crate) replication: Arc<R>, // Mandatory replication layer (use NoReplication for single-node)
    pub(crate) metrics: Arc<ShardMetrics>,
    pub(crate) readiness_tx: watch::Sender<bool>,
    pub(crate) readiness_rx: watch::Receiver<bool>,

    // Networking components (optional - can be disabled for storage-only use)
    pub(crate) z_session: Option<zenoh::Session>,
    pub(crate) inv_net_manager: Option<Arc<Mutex<InvocationNetworkManager<E>>>>,
    pub(crate) inv_offloader: Option<Arc<InvocationOffloader<E>>>,
    pub(crate) network: OnceCell<Arc<Mutex<UnifiedShardNetwork<R>>>>,

    // Event management (optional)
    pub(crate) event_manager: Option<Arc<E>>,

    // Liveliness management (optional)
    #[allow(dead_code)]
    pub(crate) liveliness_state: Option<MemberLivelinessState>,

    // Control and metadata
    pub(crate) token: CancellationToken,

    // Unified shard configuration
    pub(crate) config: ShardOptions,

    // V2 per-entry dispatcher (J2 skeleton)
    pub(crate) v2_dispatcher: Option<crate::events::V2DispatcherRef>,
}
