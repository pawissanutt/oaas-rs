use std::sync::Arc;

use oprc_pb::{InvocationResponse, ResponseStatus};
use oprc_zenoh::util::Handler;
use prost::Message;
use zenoh::query::Query;

use crate::OffloadError;

#[async_trait::async_trait]
pub trait InvocationExecutor {
    async fn invoke_fn(
        &self,
        req: oprc_pb::InvocationRequest,
    ) -> Result<oprc_pb::InvocationResponse, OffloadError>;
    async fn invoke_obj(
        &self,
        req: oprc_pb::ObjectInvocationRequest,
    ) -> Result<oprc_pb::InvocationResponse, OffloadError>;
}

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
    pub fn new(logging_prefix: String, offlorder: Arc<T>) -> Self {
        Self {
            logging_prefix,
            executor: offlorder,
        }
    }

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

async fn write_message<M: Message>(query: &Query, msg: M) {
    let byte = msg.encode_to_vec();
    if let Err(e) = query.reply(query.key_expr(), byte).await {
        write_error(query, e, ResponseStatus::SystemError as i32).await;
    }
}

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
