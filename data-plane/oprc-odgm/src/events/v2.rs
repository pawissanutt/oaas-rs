use oprc_observability::init_odgm_event_metrics;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::sync::{
    broadcast,
    mpsc::{self, Sender, error::TrySendError},
};
use tracing::{debug, info, warn};

use super::{MutAction, MutationContext};
use crate::events::TriggerProcessor;
use crate::events::types::{EventContext, EventType, TriggerExecutionContext}; // internal types
use oprc_grpc::ObjectEvent;

#[derive(Debug, Clone)]
pub struct V2QueuedEvent {
    pub seq: u64,
    pub ctx: MutationContext,
    pub summary: Option<V2Summary>,
}

#[derive(Debug, Clone)]
pub struct V2Summary {
    pub changed_total: usize,
    pub emitted: usize,
    pub sample_remaining_keys: Vec<String>,
    pub version_after: u64,
}

pub struct V2Dispatcher {
    seq: AtomicU64,
    tx: Sender<V2QueuedEvent>,
    bcast: broadcast::Sender<V2QueuedEvent>,
    // Metrics (internal counters; exposed via getters)
    emitted_events: AtomicU64,
    emitted_create: AtomicU64,
    emitted_update: AtomicU64,
    emitted_delete: AtomicU64,
    truncated_batches: AtomicU64,
    queue_drops_total: AtomicU64,
    queue_len: AtomicU64,
    trigger_processor: Option<Arc<TriggerProcessor>>,
    // Optional metrics (OpenTelemetry) instruments
    otel: Option<oprc_observability::OdgmEventMetrics>,
}

impl V2Dispatcher {
    pub fn new(queue_bound: usize, bcast_bound: usize) -> Arc<Self> {
        Self::new_with_processor(queue_bound, bcast_bound, None)
    }

    pub fn new_with_processor(
        queue_bound: usize,
        bcast_bound: usize,
        trigger_processor: Option<Arc<TriggerProcessor>>,
    ) -> Arc<Self> {
        let (tx, rx) = mpsc::channel(queue_bound);
        let (btx, _brx) = broadcast::channel(bcast_bound);
        // Try to init OTEL metrics (safe if OTEL isn't configured)
        let otel = Some(init_odgm_event_metrics("oprc-odgm"));
        let dispatcher = Arc::new(Self {
            seq: AtomicU64::new(1),
            tx,
            bcast: btx.clone(),
            emitted_events: AtomicU64::new(0),
            emitted_create: AtomicU64::new(0),
            emitted_update: AtomicU64::new(0),
            emitted_delete: AtomicU64::new(0),
            truncated_batches: AtomicU64::new(0),
            queue_drops_total: AtomicU64::new(0),
            queue_len: AtomicU64::new(0),
            trigger_processor,
            otel,
        });
        tokio::spawn(run_v2_consumer(rx, dispatcher.clone(), btx));
        dispatcher
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }

    pub fn try_send(&self, ctx: MutationContext) -> bool {
        let evt = V2QueuedEvent {
            seq: self.next_seq(),
            ctx,
            summary: None,
        };
        match self.tx.try_send(evt) {
            Ok(_) => {
                self.queue_len.fetch_add(1, Ordering::Relaxed);
                if let Some(m) = &self.otel {
                    m.queue_len.add(1, &[]);
                }
                true
            }
            Err(TrySendError::Full(_)) => {
                self.queue_drops_total.fetch_add(1, Ordering::Relaxed);
                if let Some(m) = &self.otel {
                    m.queue_drops_total.add(1, &[]);
                }
                debug!("v2 event queue full; dropping context");
                false
            }
            Err(TrySendError::Closed(_)) => {
                warn!("v2 event queue closed; dropping context");
                false
            }
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<V2QueuedEvent> {
        self.bcast.subscribe()
    }

    #[inline]
    pub fn metrics_emitted_events(&self) -> u64 {
        self.emitted_events.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn metrics_emitted_create(&self) -> u64 {
        self.emitted_create.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn metrics_emitted_update(&self) -> u64 {
        self.emitted_update.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn metrics_emitted_delete(&self) -> u64 {
        self.emitted_delete.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn metrics_truncated_batches(&self) -> u64 {
        self.truncated_batches.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn metrics_queue_drops_total(&self) -> u64 {
        self.queue_drops_total.load(Ordering::Relaxed)
    }
    #[inline]
    pub fn metrics_queue_len(&self) -> u64 {
        self.queue_len.load(Ordering::Relaxed)
    }
}

async fn run_v2_consumer(
    mut rx: mpsc::Receiver<V2QueuedEvent>,
    dispatcher: Arc<V2Dispatcher>,
    bcast: broadcast::Sender<V2QueuedEvent>,
) {
    let fanout_cap: usize = std::env::var("ODGM_MAX_BATCH_TRIGGER_FANOUT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256);
    while let Some(mut evt) = rx.recv().await {
        let changed_len = evt.ctx.changed.len();
        let mut emitted = 0usize;
        for ck in &evt.ctx.changed {
            if emitted >= fanout_cap {
                break;
            }
            emitted += 1;
            dispatcher.emitted_events.fetch_add(1, Ordering::Relaxed);
            // Basic trigger resolution skeleton (J2.4): derive prospective EventType for this changed key.
            // We don't yet have object-level ObjectEvent configuration wired for filtering; that will follow.
            let prospective_event_type = map_changed_key_to_event_type(ck);
            // Attempt trigger filtering + emission if trigger processor is configured and we can resolve targets.
            if let Some(tp) = &dispatcher.trigger_processor {
                if let Some(object_event) = evt.ctx.event_config.as_ref() {
                    let targets = collect_matching_triggers_inline(
                        &object_event,
                        &prospective_event_type,
                    );
                    if targets.is_empty() {
                        debug!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, key = %ck.key_canonical, evt_type = ?prospective_event_type, "v2 no triggers matched for key");
                    } else {
                        debug!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, key = %ck.key_canonical, evt_type = ?prospective_event_type, triggers = targets.len(), "v2 matched triggers");
                        // Build EventContext (no payload/error for data changes).
                        let event_ctx = EventContext {
                            object_id: evt.ctx.object_id.clone(),
                            class_id: evt.ctx.cls_id.clone(),
                            partition_id: evt.ctx.partition_id,
                            event_type: prospective_event_type.clone(),
                            ..Default::default()
                        };
                        for target in targets {
                            let exec_ctx = TriggerExecutionContext {
                                source_event: event_ctx.clone(),
                                target,
                            };
                            tp.execute_trigger(exec_ctx).await; // fire & forget (async inside)
                            // Increment type-specific emitted counters
                            use EventType::*;
                            match prospective_event_type {
                                DataCreate(_) => {
                                    dispatcher
                                        .emitted_create
                                        .fetch_add(1, Ordering::Relaxed);
                                    if let Some(m) = &dispatcher.otel {
                                        m.emitted_create_total.add(1, &[]);
                                    }
                                }
                                DataUpdate(_) => {
                                    dispatcher
                                        .emitted_update
                                        .fetch_add(1, Ordering::Relaxed);
                                    if let Some(m) = &dispatcher.otel {
                                        m.emitted_update_total.add(1, &[]);
                                    }
                                }
                                DataDelete(_) => {
                                    dispatcher
                                        .emitted_delete
                                        .fetch_add(1, Ordering::Relaxed);
                                    if let Some(m) = &dispatcher.otel {
                                        m.emitted_delete_total.add(1, &[]);
                                    }
                                }
                                FunctionComplete(_) | FunctionError(_) => {}
                            }
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
            if let Some(m) = &dispatcher.otel {
                m.fanout_limited_total.add(1, &[]);
            }
            // Summary fallback (log only for now)
            let sample_limit = std::env::var("ODGM_EVENT_SUMMARY_KEY_SAMPLE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(32);
            let sample: Vec<&str> = evt
                .ctx
                .changed
                .iter()
                .skip(emitted)
                .take(sample_limit)
                .map(|c| c.key_canonical.as_str())
                .collect();
            info!(seq = evt.seq, cls = %evt.ctx.cls_id, partition = evt.ctx.partition_id, obj = %evt.ctx.object_id, version_after = evt.ctx.version_after, emitted = emitted, changed_total = changed_len, sample_remaining_keys = %sample.join(","), "v2 summary fallback (fanout truncated)");
            // Attach structured summary to event for in-process consumers/tests
            evt.summary = Some(V2Summary {
                changed_total: changed_len,
                emitted,
                sample_remaining_keys: sample
                    .into_iter()
                    .map(|s| s.to_string())
                    .collect(),
                version_after: evt.ctx.version_after,
            });
        }
        let _ = bcast.send(evt);
        // Decrement queue length after processing one event
        dispatcher
            .queue_len
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            })
            .ok();
        if let Some(m) = &dispatcher.otel {
            m.queue_len.add(-1, &[]);
        }
    }
}

// Map a changed key + action into an internal EventType variant (numeric vs string).
// This is a lightweight helper for the trigger resolution skeleton; actual trigger filtering will
// use this mapping plus object-level ObjectEvent config in a subsequent patch.
fn map_changed_key_to_event_type(ck: &super::ChangedKey) -> EventType {
    match ck.action {
        MutAction::Create => EventType::DataCreate(ck.key_canonical.clone()),
        MutAction::Update => EventType::DataUpdate(ck.key_canonical.clone()),
        MutAction::Delete => EventType::DataDelete(ck.key_canonical.clone()),
    }
}

// Inline adaptation of legacy collect_matching_triggers for per-entry EventType.
fn collect_matching_triggers_inline(
    object_event: &ObjectEvent,
    event_type: &EventType,
) -> Vec<oprc_grpc::TriggerTarget> {
    use EventType::*;
    match event_type {
        DataCreate(key) => object_event
            .data_trigger
            .get(key)
            .map(|t| t.on_create.clone())
            .unwrap_or_default(),
        DataUpdate(key) => object_event
            .data_trigger
            .get(key)
            .map(|t| t.on_update.clone())
            .unwrap_or_default(),
        DataDelete(key) => object_event
            .data_trigger
            .get(key)
            .map(|t| t.on_delete.clone())
            .unwrap_or_default(),
        // FunctionComplete / FunctionError not expected in per-entry path here.
        FunctionComplete(_) | FunctionError(_) => Vec::new(),
    }
}

// Placeholder loader for ObjectEvent configuration until granular persistence wiring is added.

pub type V2DispatcherRef = Arc<V2Dispatcher>;
