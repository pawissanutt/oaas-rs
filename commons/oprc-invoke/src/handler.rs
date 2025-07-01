use std::sync::Arc;

use oprc_pb::{InvocationResponse, ResponseStatus};
use oprc_zenoh::util::Handler;
use prost::Message;
use zenoh::{query::Query, sample::Sample};

use crate::OffloadError;

/// Trait for executing function and object invocations.
/// This trait abstracts the actual execution logic from the Zenoh transport layer.
#[async_trait::async_trait]
pub trait InvocationExecutor {
    /// Execute a stateless function invocation
    async fn invoke_fn(
        &self,
        req: oprc_pb::InvocationRequest,
    ) -> Result<oprc_pb::InvocationResponse, OffloadError>;

    /// Execute an object method invocation
    async fn invoke_obj(
        &self,
        req: oprc_pb::ObjectInvocationRequest,
    ) -> Result<oprc_pb::InvocationResponse, OffloadError>;
}

/// Handler for synchronous invocations over Zenoh using GET/Queryable pattern.
/// This handler processes incoming queries for function and object method invocations
/// and sends responses back through the same query channel.
pub struct InvocationZenohHandler<T: InvocationExecutor + Send + Sync> {
    logging_prefix: String,
    executor: Arc<T>,
}

impl<T: InvocationExecutor + Send + Sync> Clone for InvocationZenohHandler<T> {
    fn clone(&self) -> Self {
        Self {
            logging_prefix: self.logging_prefix.clone(),
            executor: self.executor.clone(),
        }
    }
}

impl<T: InvocationExecutor + Send + Sync> InvocationZenohHandler<T> {
    /// Create a new synchronous invocation handler
    pub fn new(logging_prefix: String, executor: Arc<T>) -> Self {
        Self {
            logging_prefix,
            executor,
        }
    }

    /// Handle stateless function invocation requests
    async fn handle_invoke_fn(&self, query: Query) {
        match decode(&query) {
            Ok(req) => {
                match self.executor.invoke_fn(req).await {
                    Ok(resp) => {
                        write_message(&query, resp).await;
                    }
                    Err(OffloadError::GrpcError(s)) => {
                        write_error(&query, s, ResponseStatus::AppError as i32)
                            .await;
                    }
                    Err(e) => {
                        write_error(
                            &query,
                            e,
                            ResponseStatus::SystemError as i32,
                        )
                        .await;
                    }
                };
            }
            Err(e) => {
                write_error(&query, e, ResponseStatus::InvalidRequest as i32)
                    .await;
            }
        }
    }

    /// Handle object method invocation requests
    async fn handle_invoke_obj(&self, query: Query) {
        match decode(&query) {
            Ok(req) => {
                match self.executor.invoke_obj(req).await {
                    Ok(resp) => {
                        write_message(&query, resp).await;
                    }
                    Err(OffloadError::GrpcError(s)) => {
                        write_error(&query, s, ResponseStatus::AppError as i32)
                            .await;
                    }
                    Err(e) => {
                        write_error(
                            &query,
                            e,
                            ResponseStatus::SystemError as i32,
                        )
                        .await;
                    }
                };
            }
            Err(e) => {
                write_error(&query, e, ResponseStatus::InvalidRequest as i32)
                    .await;
                return;
            }
        }
    }
}

#[async_trait::async_trait]
impl<T> Handler<Query> for InvocationZenohHandler<T>
where
    T: InvocationExecutor + Send + Sync + 'static,
{
    async fn handle(&self, query: Query) {
        let is_object = match query.key_expr().split("/").skip(3).next() {
            Some(path) => path == "objects",
            None => {
                return;
            }
        };
        tracing::debug!(
            "{}: received invocation, '{}'",
            self.logging_prefix,
            query.key_expr()
        );
        if is_object {
            self.handle_invoke_obj(query).await;
        } else {
            self.handle_invoke_fn(query).await;
        }
    }
}

/// Handler for asynchronous invocations over Zenoh using PUT/Subscriber pattern.
/// This handler processes incoming samples (PUT messages) for async function and object method invocations.
/// Unlike synchronous invocations, async invocations don't send immediate responses but rather
/// store or publish results for later retrieval.
pub struct AsyncInvocationHandler<T: InvocationExecutor + Send + Sync> {
    logging_prefix: String,
    executor: Arc<T>,
}

impl<T: InvocationExecutor + Send + Sync> Clone for AsyncInvocationHandler<T> {
    fn clone(&self) -> Self {
        Self {
            logging_prefix: self.logging_prefix.clone(),
            executor: self.executor.clone(),
        }
    }
}

impl<T: InvocationExecutor + Send + Sync> AsyncInvocationHandler<T> {
    /// Create a new asynchronous invocation handler
    pub fn new(logging_prefix: String, executor: Arc<T>) -> Self {
        Self {
            logging_prefix,
            executor,
        }
    }

    /// Handle asynchronous stateless function invocation requests
    /// Expected key format: oprc/<class>/<partition>/invokes/<method_id>/async/<invocation_id>
    async fn handle_async_invoke_fn(&self, sample: Sample) {
        let key_expr = sample.key_expr().as_str();

        // Extract invocation_id from key_expr like: oprc/class/partition/invokes/method/async/invocation_id
        let parts: Vec<&str> = key_expr.split('/').collect();
        if parts.len() < 6 {
            tracing::warn!(
                "{}: Invalid async invocation key format: {}",
                self.logging_prefix,
                key_expr
            );
            return;
        }

        let invocation_id = parts[parts.len() - 1];
        tracing::debug!(
            "{}: Handling async invocation {}",
            self.logging_prefix,
            invocation_id
        );

        match decode_sample(&sample) {
            Ok(req) => {
                match self.executor.invoke_fn(req).await {
                    Ok(_resp) => {
                        tracing::info!(
                            "{}: Async invocation {} completed successfully",
                            self.logging_prefix,
                            invocation_id
                        );
                        // TODO: Store result for later retrieval or publish to result topic
                    }
                    Err(OffloadError::GrpcError(s)) => {
                        tracing::error!(
                            "{}: Async invocation {} failed with gRPC error: {}",
                            self.logging_prefix,
                            invocation_id,
                            s
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "{}: Async invocation {} failed with system error: {}",
                            self.logging_prefix,
                            invocation_id,
                            e
                        );
                    }
                };
            }
            Err(e) => {
                tracing::error!(
                    "{}: Failed to decode async invocation {}: {}",
                    self.logging_prefix,
                    invocation_id,
                    e
                );
            }
        }
    }

    /// Handle asynchronous object method invocation requests
    /// Expected key format: oprc/<class>/<partition>/objects/<object_id>/invokes/<method_id>/async/<invocation_id>
    async fn handle_async_invoke_obj(&self, sample: Sample) {
        let key_expr = sample.key_expr().as_str();

        // Extract invocation_id from key_expr like: oprc/class/partition/objects/object_id/invokes/method/async/invocation_id
        let parts: Vec<&str> = key_expr.split('/').collect();
        if parts.len() < 8 {
            tracing::warn!(
                "{}: Invalid async object invocation key format: {}",
                self.logging_prefix,
                key_expr
            );
            return;
        }

        let invocation_id = parts[parts.len() - 1];
        tracing::debug!(
            "{}: Handling async object invocation {}",
            self.logging_prefix,
            invocation_id
        );

        match decode_sample(&sample) {
            Ok(req) => {
                match self.executor.invoke_obj(req).await {
                    Ok(_resp) => {
                        tracing::info!(
                            "{}: Async object invocation {} completed successfully",
                            self.logging_prefix,
                            invocation_id
                        );
                        // TODO: Store result for later retrieval or publish to result topic
                    }
                    Err(OffloadError::GrpcError(s)) => {
                        tracing::error!(
                            "{}: Async object invocation {} failed with gRPC error: {}",
                            self.logging_prefix,
                            invocation_id,
                            s
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            "{}: Async object invocation {} failed with system error: {}",
                            self.logging_prefix,
                            invocation_id,
                            e
                        );
                    }
                };
            }
            Err(e) => {
                tracing::error!(
                    "{}: Failed to decode async object invocation {}: {}",
                    self.logging_prefix,
                    invocation_id,
                    e
                );
            }
        }
    }
}

#[async_trait::async_trait]
impl<T> Handler<Sample> for AsyncInvocationHandler<T>
where
    T: InvocationExecutor + Send + Sync + 'static,
{
    async fn handle(&self, sample: Sample) {
        let key_expr = sample.key_expr().as_str();
        let is_object = key_expr.contains("/objects/");

        tracing::debug!(
            "{}: received async invocation sample, '{}'",
            self.logging_prefix,
            key_expr
        );

        if is_object {
            self.handle_async_invoke_obj(sample).await;
        } else {
            self.handle_async_invoke_fn(sample).await;
        }
    }
}

// Helper functions for message processing

/// Decode a protobuf message from a Zenoh query payload
fn decode<M>(query: &Query) -> Result<M, String>
where
    M: Message + Default,
{
    match query.payload() {
        Some(payload) => match M::decode(payload.to_bytes().as_ref()) {
            Ok(msg) => Ok(msg),
            Err(e) => Err(e.to_string()),
        },
        None => Err("Payload must not be empty".into()),
    }
}

/// Decode a protobuf message from a Zenoh sample payload  
fn decode_sample<M>(sample: &Sample) -> Result<M, String>
where
    M: Message + Default,
{
    match M::decode(sample.payload().to_bytes().as_ref()) {
        Ok(msg) => Ok(msg),
        Err(e) => Err(e.to_string()),
    }
}

/// Write a successful response message back to the query sender
async fn write_message<M: Message>(query: &Query, msg: M) {
    let byte = msg.encode_to_vec();
    if let Err(e) = query.reply(query.key_expr(), byte).await {
        write_error(query, e, ResponseStatus::SystemError as i32).await;
    }
}

/// Write an error response back to the query sender
async fn write_error<E: ToString>(query: &Query, e: E, status: i32) {
    let resp = InvocationResponse {
        payload: Some(e.to_string().into_bytes().into()),
        status,
        ..Default::default()
    };
    if let Err(e) = query.reply(query.key_expr(), resp.encode_to_vec()).await {
        tracing::warn!("Failed to reply error: {:?}", e);
    }
}
