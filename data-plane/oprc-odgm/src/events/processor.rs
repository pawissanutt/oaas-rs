use crate::events::config::EventConfig;
use crate::events::types::{
    TriggerExecutionContext, create_trigger_payload, serialize_trigger_payload,
};
use once_cell::sync::OnceCell;
use oprc_grpc::{InvocationRequest, ObjectInvocationRequest};
use prost::Message;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tracing::{debug, error};
use zenoh::Session;
use zenoh::bytes::ZBytes;

pub struct TriggerProcessor {
    // Zenoh session for async invocation
    z_session: Session,
    // Configuration for trigger execution
    config: EventConfig,
    // Optional test tap (records executed triggers) - initialized when ODGM_TRIGGER_TEST_TAP=1
    test_tap: Option<Arc<Mutex<Vec<TriggerExecutionContext>>>>,
    // Global/shared failure counter reference (used for metrics/tests)
    failures_total: Arc<AtomicU64>,
}

impl TriggerProcessor {
    pub fn new(z_session: Session, config: EventConfig) -> Self {
        let test_tap = if std::env::var("ODGM_TRIGGER_TEST_TAP").ok().as_deref()
            == Some("1")
        {
            Some(
                GLOBAL_TRIGGER_TEST_TAP
                    .get_or_init(|| Arc::new(Mutex::new(Vec::new())))
                    .clone(),
            )
        } else {
            None
        };
        let failures_total = GLOBAL_TRIGGER_FAILURES_TOTAL
            .get_or_init(|| Arc::new(AtomicU64::new(0)))
            .clone();
        Self {
            z_session,
            config,
            test_tap,
            failures_total,
        }
    }

    /// Execute a trigger using Zenoh's fire-and-forget async invocation
    pub async fn execute_trigger(&self, context: TriggerExecutionContext) {
        debug!(
            "Executing trigger via Zenoh async invocation: {:?} -> {:?}",
            context.source_event.event_type, context.target
        );

        // Prepare trigger payload with event context
        let trigger_payload = self.prepare_trigger_payload(&context);

        // Generate unique invocation ID for tracking
        let invocation_id = nanoid::nanoid!();

        // Execute fire-and-forget async invocation via Zenoh PUT
        let context_clone_for_tap = context.clone();
        match self
            .execute_async_invocation(context, trigger_payload, &invocation_id)
            .await
        {
            Ok(_) => {
                debug!(
                    "Trigger dispatched successfully: invocation_id={}",
                    invocation_id
                );
                if let Some(tap) = &self.test_tap {
                    let mut guard = tap.lock().await;
                    guard.push(context_clone_for_tap);
                }
            }
            Err(e) => {
                error!(
                    "Failed to dispatch trigger: invocation_id={}, error={:?}",
                    invocation_id, e
                );
                // Increment failure metrics counter
                self.failures_total.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    async fn execute_async_invocation(
        &self,
        context: TriggerExecutionContext,
        payload: Vec<u8>,
        invocation_id: &str,
    ) -> Result<(), String> {
        // Test-only failure injection: if set, simulate a dispatch failure.
        if std::env::var("ODGM_FORCE_TRIGGER_PUBLISH_FAIL")
            .ok()
            .as_deref()
            == Some("1")
        {
            return Err("forced publish failure".to_string());
        }
        // Construct Zenoh key expression for async invocation
        let key_expr = self.build_async_key_expr(&context, invocation_id);

        // Build the appropriate invocation request
        let encoded_payload =
            self.build_invocation_request(&context, payload)?;

        // Fire-and-forget PUT via Zenoh (async invocation)
        // This uses Zenoh's async invocation pattern where:
        // 1. PUT operation dispatches the invocation request
        // 2. The invocation is handled asynchronously by the target partition
        // 3. No response is expected (true fire-and-forget)
        // 4. Results are ignored for maximum performance
        self.z_session
            .put(key_expr, ZBytes::from(encoded_payload))
            .await
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn build_async_key_expr(
        &self,
        context: &TriggerExecutionContext,
        invocation_id: &str,
    ) -> String {
        if let Some(id) = &context.target.object_id {
            // Async object method invocation key using string ID
            format!(
                "oprc/{}/{}/objects/{}/async/{}/{}",
                context.target.cls_id,
                context.target.partition_id,
                id,
                context.target.fn_id,
                invocation_id
            )
        } else {
            // Async stateless function invocation key
            format!(
                "oprc/{}/{}/async/{}/{}",
                context.target.cls_id,
                context.target.partition_id,
                context.target.fn_id,
                invocation_id
            )
        }
    }

    fn build_invocation_request(
        &self,
        context: &TriggerExecutionContext,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        if let Some(object_id) = &context.target.object_id {
            let request = ObjectInvocationRequest {
                partition_id: context.target.partition_id,
                object_id: Some(object_id.clone()),
                cls_id: context.target.cls_id.clone(),
                fn_id: context.target.fn_id.clone(),
                options: context.target.req_options.clone(),
                payload,
            };
            let mut buf = Vec::new();
            request
                .encode(&mut buf)
                .map_err(|e| format!("Failed to encode request: {}", e))?;
            Ok(buf)
        } else {
            let request = InvocationRequest {
                partition_id: context.target.partition_id,
                cls_id: context.target.cls_id.clone(),
                fn_id: context.target.fn_id.clone(),
                options: context.target.req_options.clone(),
                payload,
            };
            let mut buf = Vec::new();
            request
                .encode(&mut buf)
                .map_err(|e| format!("Failed to encode request: {}", e))?;
            Ok(buf)
        }
    }

    fn prepare_trigger_payload(
        &self,
        context: &TriggerExecutionContext,
    ) -> Vec<u8> {
        // Create structured payload using the protobuf-generated TriggerPayload
        // Note: Target information (class, partition, function) is implicit in the
        // invocation context, so we only need to send event information
        let payload = create_trigger_payload(context);

        // Serialize using the configured format (JSON by default)
        serialize_trigger_payload(&payload, self.config.payload_format)
    }
}

// Global test tap (shared across processors in tests)
static GLOBAL_TRIGGER_TEST_TAP: OnceCell<
    Arc<Mutex<Vec<TriggerExecutionContext>>>,
> = OnceCell::new();

/// Drain and return recorded triggers (test utility). Returns None if tap disabled.
pub async fn drain_trigger_test_tap() -> Option<Vec<TriggerExecutionContext>> {
    if let Some(tap) = GLOBAL_TRIGGER_TEST_TAP.get() {
        let mut guard = tap.lock().await;
        let out = guard.drain(..).collect();
        Some(out)
    } else {
        None
    }
}

// Global failure counter (shared across processors) for metrics/tests
static GLOBAL_TRIGGER_FAILURES_TOTAL: OnceCell<Arc<AtomicU64>> =
    OnceCell::new();

/// Read and reset the total number of trigger dispatch failures (test utility)
pub fn read_and_reset_trigger_failures_total() -> u64 {
    if let Some(counter) = GLOBAL_TRIGGER_FAILURES_TOTAL.get() {
        let v = counter.load(Ordering::Relaxed);
        counter.store(0, Ordering::Relaxed);
        v
    } else {
        0
    }
}

/// Get the current number of trigger dispatch failures (without resetting)
pub fn trigger_failures_total() -> u64 {
    GLOBAL_TRIGGER_FAILURES_TOTAL
        .get()
        .map(|c| c.load(Ordering::Relaxed))
        .unwrap_or(0)
}
