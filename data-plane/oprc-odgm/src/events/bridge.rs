use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{
    broadcast,
    mpsc::{Sender, error::TrySendError},
};
use tracing::{debug, warn};

/// Bridge (J0) summary event representing a single batch/object mutation.
#[derive(Debug, Clone)]
pub struct BridgeSummaryEvent {
    pub cls_id: String,
    pub partition_id: u16,
    pub object_id_str: String, // canonical normalized id (string form)
    pub version: u64,
    pub changed_keys: Vec<String>,
    pub timestamp_ms: u64,
    pub seq: u64,
}

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    pub enabled: bool,
    pub sample_limit: usize,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            enabled: is_bridge_enabled_from_env(),
            sample_limit: bridge_sample_limit_from_env(),
        }
    }
}

fn is_bridge_enabled_from_env() -> bool {
    // Hard override: if V2 pipeline is explicitly enabled, disable bridge.
    if std::env::var("ODGM_EVENT_PIPELINE_V2")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false)
    {
        return false;
    }
    std::env::var("ODGM_EVENT_PIPELINE_BRIDGE")
        .ok()
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(true)
}

fn bridge_sample_limit_from_env() -> usize {
    std::env::var("ODGM_EVENT_SUMMARY_KEY_SAMPLE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(32)
}

/// Simple dispatcher that converts bridge summary events into the standard TriggerPayload
/// using a minimal context map. It relies on an injected publish function so we avoid
/// pulling in Zenoh specifics here.
pub struct BridgeDispatcher {
    seq: AtomicU64,
    tx: Sender<BridgeSummaryEvent>,
    /// Broadcast channel for minimal subscription (J0-2)
    bcast: broadcast::Sender<BridgeSummaryEvent>,
}

impl BridgeDispatcher {
    pub fn new(
        tx: Sender<BridgeSummaryEvent>,
        bcast: broadcast::Sender<BridgeSummaryEvent>,
    ) -> Self {
        Self {
            seq: AtomicU64::new(1),
            tx,
            bcast,
        }
    }

    pub fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    pub fn try_send(&self, mut evt: BridgeSummaryEvent) -> bool {
        if evt.seq == 0 {
            evt.seq = self.next_seq();
        }
        match self.tx.try_send(evt) {
            Ok(_) => true,
            Err(TrySendError::Full(_e)) => {
                debug!("bridge event queue full; dropping summary event");
                false
            }
            Err(TrySendError::Closed(_e)) => {
                warn!("bridge event queue closed; dropping summary event");
                false
            }
        }
    }

    /// Subscribe to bridge summary events (best-effort). Returns a broadcast receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<BridgeSummaryEvent> {
        self.bcast.subscribe()
    }
}

/// Build a bridge summary event given mutation info; changed_keys already canonical.
pub fn build_bridge_event(
    cls_id: &str,
    partition_id: u16,
    object_id_str: &str,
    version: u64,
    changed_keys: Vec<String>,
    dispatcher: &BridgeDispatcher,
) -> BridgeSummaryEvent {
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    BridgeSummaryEvent {
        cls_id: cls_id.to_string(),
        partition_id,
        object_id_str: object_id_str.to_string(),
        version,
        changed_keys,
        timestamp_ms,
        seq: dispatcher.next_seq(),
    }
}

/// Render a textual sample representation to embed in context map or logs.
pub fn summarize_keys(keys: &[String], sample_limit: usize) -> (String, bool) {
    if keys.is_empty() {
        return (String::new(), false);
    }
    let truncated = keys.len() > sample_limit;
    let sample: Vec<&String> = keys.iter().take(sample_limit).collect();
    (
        sample
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(","),
        truncated,
    )
}

/// Run the dispatcher consumer loop. For J0 we simply log; wiring to actual Zenoh publish
/// (object-level event topic) can be added later.
pub async fn run_bridge_consumer(
    mut rx: tokio::sync::mpsc::Receiver<BridgeSummaryEvent>,
    cfg: BridgeConfig,
    bcast: broadcast::Sender<BridgeSummaryEvent>,
) {
    while let Some(evt) = rx.recv().await {
        if !cfg.enabled {
            continue;
        }
        let (sample, truncated) =
            summarize_keys(&evt.changed_keys, cfg.sample_limit);
        debug!(
            class = %evt.cls_id,
            partition = evt.partition_id,
            object = %evt.object_id_str,
            version = evt.version,
            changed_total = evt.changed_keys.len(),
            sample_keys = %sample,
            truncated = truncated,
            seq = evt.seq,
            "bridge summary event"
        );
        // Broadcast for minimal subscription listeners; ignore if lagging.
        let _ = bcast.send(evt);
    }
}

pub type BridgeDispatcherRef = Arc<BridgeDispatcher>;
