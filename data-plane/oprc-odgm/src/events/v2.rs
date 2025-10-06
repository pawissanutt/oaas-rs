use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tokio::sync::{mpsc::{self, Sender, error::TrySendError}, broadcast};
use tracing::{debug, warn, info};

use super::{MutationContext, MutAction};
use crate::events::types::{EventType, EventContext, TriggerExecutionContext}; // internal types
use crate::events::TriggerProcessor;
use oprc_grpc::ObjectEvent;

#[derive(Debug, Clone)]
pub struct V2QueuedEvent {
    pub seq: u64,
    pub ctx: MutationContext,
}

pub struct V2Dispatcher {
    seq: AtomicU64,
    tx: Sender<V2QueuedEvent>,
    bcast: broadcast::Sender<V2QueuedEvent>,
    emitted_events: AtomicU64,
    truncated_batches: AtomicU64,
    trigger_processor: Option<Arc<TriggerProcessor>>,
}

impl V2Dispatcher {
    pub fn new(queue_bound: usize, bcast_bound: usize) -> Arc<Self> { Self::new_with_processor(queue_bound, bcast_bound, None) }

    pub fn new_with_processor(queue_bound: usize, bcast_bound: usize, trigger_processor: Option<Arc<TriggerProcessor>>) -> Arc<Self> {
        let (tx, rx) = mpsc::channel(queue_bound);
        let (btx, _brx) = broadcast::channel(bcast_bound);
        let dispatcher = Arc::new(Self { seq: AtomicU64::new(1), tx, bcast: btx.clone(), emitted_events: AtomicU64::new(0), truncated_batches: AtomicU64::new(0), trigger_processor });
        tokio::spawn(run_v2_consumer(rx, dispatcher.clone(), btx));
        dispatcher
    }

    fn next_seq(&self) -> u64 { self.seq.fetch_add(1, Ordering::Relaxed) }

    pub fn try_send(&self, ctx: MutationContext) -> bool {
        let evt = V2QueuedEvent { seq: self.next_seq(), ctx };
        match self.tx.try_send(evt) {
            Ok(_) => true,
            Err(TrySendError::Full(_)) => { debug!("v2 event queue full; dropping context"); false },
            Err(TrySendError::Closed(_)) => { warn!("v2 event queue closed; dropping context"); false },
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<V2QueuedEvent> { self.bcast.subscribe() }
}

async fn run_v2_consumer(mut rx: mpsc::Receiver<V2QueuedEvent>, dispatcher: Arc<V2Dispatcher>, bcast: broadcast::Sender<V2QueuedEvent>) {
    let fanout_cap: usize = std::env::var("ODGM_MAX_BATCH_TRIGGER_FANOUT").ok().and_then(|v| v.parse().ok()).unwrap_or(256);
    while let Some(evt) = rx.recv().await {
        let changed_len = evt.ctx.changed.len();
        let mut emitted = 0usize;
        for ck in &evt.ctx.changed {
            if emitted >= fanout_cap { break; }
            emitted += 1;
            dispatcher.emitted_events.fetch_add(1, Ordering::Relaxed);
            // Basic trigger resolution skeleton (J2.4): derive prospective EventType for this changed key.
            // We don't yet have object-level ObjectEvent configuration wired for filtering; that will follow.
            let prospective_event_type = map_changed_key_to_event_type(ck);
            // Attempt trigger filtering + emission if trigger processor is configured and we can resolve targets.
            if let Some(tp) = &dispatcher.trigger_processor {
                if let Some(object_event) = evt.ctx.event_config.as_ref() {
                    let targets = collect_matching_triggers_inline(&object_event, &prospective_event_type);
                    if targets.is_empty() {
                        debug!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, key = %ck.key_canonical, evt_type = ?prospective_event_type, "v2 no triggers matched for key");
                    } else {
                        debug!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, key = %ck.key_canonical, evt_type = ?prospective_event_type, triggers = targets.len(), "v2 matched triggers");
                        // Build EventContext (no payload/error for data changes).
                        let object_id_u64 = parse_object_id_to_u64(&evt.ctx.object_id);
                        let event_ctx = EventContext { object_id: object_id_u64, class_id: evt.ctx.cls_id.clone(), partition_id: evt.ctx.partition_id, event_type: prospective_event_type.clone(), payload: None, error_message: None };
                        for target in targets {
                            let exec_ctx = TriggerExecutionContext { source_event: event_ctx.clone(), target };
                            tp.execute_trigger(exec_ctx).await; // fire & forget (async inside)
                        }
                    }
                } else {
                    debug!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, key = %ck.key_canonical, evt_type = ?prospective_event_type, "v2 object event config unavailable (skip triggers)");
                }
            } else {
                debug!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, version_after = evt.ctx.version_after, action = ?ck.action, key = %ck.key_canonical, evt_type = ?prospective_event_type, "v2 per-entry change (no trigger processor)");
            }
        }
        if emitted < changed_len {
            dispatcher.truncated_batches.fetch_add(1, Ordering::Relaxed);
            // Summary fallback (log only for now)
            let sample_limit = std::env::var("ODGM_EVENT_SUMMARY_KEY_SAMPLE").ok().and_then(|v| v.parse().ok()).unwrap_or(32);
            let sample: Vec<&str> = evt.ctx.changed.iter().skip(emitted).take(sample_limit).map(|c| c.key_canonical.as_str()).collect();
            info!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, version_after = evt.ctx.version_after, emitted = emitted, changed_total = changed_len, sample_remaining_keys = %sample.join(","), "v2 summary fallback (fanout truncated)");
        }
        let _ = bcast.send(evt);
    }
}

// Map a changed key + action into an internal EventType variant (numeric vs string).
// This is a lightweight helper for the trigger resolution skeleton; actual trigger filtering will
// use this mapping plus object-level ObjectEvent config in a subsequent patch.
fn map_changed_key_to_event_type(ck: &super::ChangedKey) -> EventType {
    // Try numeric parse; fall back to string variant.
    if let Ok(num) = ck.key_canonical.parse::<u32>() {
        match ck.action {
            MutAction::Create => EventType::DataCreate(num),
            MutAction::Update => EventType::DataUpdate(num),
            MutAction::Delete => EventType::DataDelete(num),
        }
    } else {
        match ck.action {
            MutAction::Create => EventType::DataCreateStr(ck.key_canonical.clone()),
            MutAction::Update => EventType::DataUpdateStr(ck.key_canonical.clone()),
            MutAction::Delete => EventType::DataDeleteStr(ck.key_canonical.clone()),
        }
    }
}

// Inline adaptation of legacy collect_matching_triggers for per-entry EventType.
fn collect_matching_triggers_inline(object_event: &ObjectEvent, event_type: &EventType) -> Vec<oprc_grpc::TriggerTarget> {
    use EventType::*;
    match event_type {
        DataCreate(fid) => object_event.data_trigger.get(fid).map(|t| t.on_create.clone()).unwrap_or_default(),
        DataUpdate(fid) => object_event.data_trigger.get(fid).map(|t| t.on_update.clone()).unwrap_or_default(),
        DataDelete(fid) => object_event.data_trigger.get(fid).map(|t| t.on_delete.clone()).unwrap_or_default(),
        DataCreateStr(k) => object_event.data_trigger_str.get(k).map(|t| t.on_create.clone()).unwrap_or_default(),
        DataUpdateStr(k) => object_event.data_trigger_str.get(k).map(|t| t.on_update.clone()).unwrap_or_default(),
        DataDeleteStr(k) => object_event.data_trigger_str.get(k).map(|t| t.on_delete.clone()).unwrap_or_default(),
        // FunctionComplete / FunctionError not expected in per-entry path here.
        FunctionComplete(_) | FunctionError(_) => Vec::new(),
    }
}

// Placeholder: resolve object_id string to u64 (numeric IDs) else 0; will evolve with string-ID support semantics.
fn parse_object_id_to_u64(object_id: &str) -> u64 { object_id.parse().unwrap_or(0) }

// Placeholder loader for ObjectEvent configuration until granular persistence wiring is added.

pub type V2DispatcherRef = Arc<V2Dispatcher>;
