use crate::events::config::EventConfig;
use crate::events::types::{
    TriggerExecutionContext, create_trigger_payload, serialize_trigger_payload,
};
use oprc_pb::{InvocationRequest, ObjectInvocationRequest};
use prost::Message;
use tracing::{debug, error};
use uuid::Uuid;
use zenoh::Session;
use zenoh::bytes::ZBytes;

pub struct TriggerProcessor {
    // Zenoh session for async invocation
    z_session: Session,
    // Configuration for trigger execution
    config: EventConfig,
}

impl TriggerProcessor {
    pub fn new(z_session: Session, config: EventConfig) -> Self {
        Self { z_session, config }
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
        let invocation_id = Uuid::new_v4().to_string();

        // Execute fire-and-forget async invocation via Zenoh PUT
        match self
            .execute_async_invocation(context, trigger_payload, &invocation_id)
            .await
        {
            Ok(_) => {
                debug!(
                    "Trigger dispatched successfully: invocation_id={}",
                    invocation_id
                );
            }
            Err(e) => {
                error!(
                    "Failed to dispatch trigger: invocation_id={}, error={:?}",
                    invocation_id, e
                );
            }
        }
    }

    async fn execute_async_invocation(
        &self,
        context: TriggerExecutionContext,
        payload: Vec<u8>,
        invocation_id: &str,
    ) -> Result<(), String> {
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
        match context.target.object_id {
            Some(target_object_id) => {
                // Async object method invocation key
                format!(
                    "oprc/{}/{}/objects/{}/async/{}/{}",
                    context.target.cls_id,
                    context.target.partition_id,
                    target_object_id,
                    context.target.fn_id,
                    invocation_id
                )
            }
            None => {
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
    }

    fn build_invocation_request(
        &self,
        context: &TriggerExecutionContext,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, String> {
        match context.target.object_id {
            Some(target_object_id) => {
                let request = ObjectInvocationRequest {
                    partition_id: context.target.partition_id,
                    object_id: target_object_id,
                    cls_id: context.target.cls_id.clone(),
                    fn_id: context.target.fn_id.clone(),
                    options: context.target.req_options.clone(),
                    payload,
                };
                Ok(request.encode_to_vec())
            }
            None => {
                let request = InvocationRequest {
                    partition_id: context.target.partition_id,
                    cls_id: context.target.cls_id.clone(),
                    fn_id: context.target.fn_id.clone(),
                    options: context.target.req_options.clone(),
                    payload,
                };
                Ok(request.encode_to_vec())
            }
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
