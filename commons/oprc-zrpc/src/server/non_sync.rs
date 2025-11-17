use std::{error::Error, marker::PhantomData};

use anyerror::AnyError;
use flume::Receiver;
use tracing::{debug, error, info, warn};
use zenoh::query::{Query, Queryable};

use crate::{ZrpcServerError, ZrpcSystemError, ZrpcTypeConfig};

use super::ServerConfig;
use crate::msg::MsgSerde;

#[async_trait::async_trait]
pub trait ZrpcNonSyncServiceHander<C: ZrpcTypeConfig>: Send + Clone {
    async fn handle(&mut self, req: C::In) -> Result<C::Out, C::Err>;
}

pub struct ZrpcNonSyncService<T, C>
where
    C: ZrpcTypeConfig,
    T: ZrpcNonSyncServiceHander<C> + 'static,
{
    z_session: zenoh::Session,
    handler: T,
    config: ServerConfig,
    queryable: Option<Queryable<Receiver<Query>>>,
    _type: PhantomData<C>,
}

impl<T, C> Clone for ZrpcNonSyncService<T, C>
where
    C: ZrpcTypeConfig,
    T: ZrpcNonSyncServiceHander<C> + 'static,
{
    fn clone(&self) -> Self {
        Self {
            z_session: self.z_session.clone(),
            handler: self.handler.clone(),
            config: self.config.clone(),
            queryable: None,
            _type: self._type.clone(),
        }
    }
}

impl<T, C> ZrpcNonSyncService<T, C>
where
    C: ZrpcTypeConfig,
    T: ZrpcNonSyncServiceHander<C> + Send,
{
    pub fn new(
        z_session: zenoh::Session,
        config: ServerConfig,
        handler: T,
    ) -> Self {
        ZrpcNonSyncService {
            z_session,
            handler: handler,
            config,
            queryable: None,
            _type: PhantomData,
        }
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let key = if self.config.accept_subfix {
            format!("{}/**", self.config.service_id)
        } else {
            self.config.service_id.clone()
        };
        let channels = if self.config.bound_channel == 0 {
            info!("RPC server '{}': use unbounded channel", key);
            flume::unbounded()
        } else {
            info!(
                "RPC server '{}': use bounded channel({})",
                key, self.config.bound_channel
            );
            flume::bounded(self.config.bound_channel as usize)
        };
        info!("RPC server '{}': registering", key);
        let queryable = self
            .z_session
            .declare_queryable(key.clone())
            .complete(self.config.complete)
            .with(channels)
            .await?;

        for _ in 0..self.config.concurrency {
            // let local_token = self.token.clone();
            let local_rx = queryable.handler().clone();
            let handler = self.handler.clone();
            let ke = key.clone();
            let conf = self.config.clone();
            tokio::spawn(async move {
                let mut handler = handler;
                loop {
                    match local_rx.recv_async().await {
                        Ok(query) => {
                            handler = Self::handle(handler, query, &conf).await
                        }
                        Err(err) => {
                            debug!("RPC server '{}': error: {}", ke, err,);
                            break;
                        }
                    }
                }
            });
        }
        self.queryable = Some(queryable);
        Ok(())
    }

    async fn handle(handler: T, query: Query, conf: &ServerConfig) -> T {
        if let Some(payload) = query.payload() {
            match C::InSerde::from_zbyte(payload) {
                Ok(data) => Self::run_handler(handler, query, data, conf).await,
                Err(err) => {
                    let zse = ZrpcSystemError::DecodeError(AnyError::new(&err));
                    Self::write_error(ZrpcServerError::SystemError(zse), query)
                        .await;
                    handler
                }
            }
        } else {
            warn!(
                "RPC server: receive rpc '{}' without payload",
                query.key_expr()
            );
            handler
        }
    }

    async fn run_handler(
        handler: T,
        query: Query,
        payload: C::In,
        conf: &ServerConfig,
    ) -> T {
        let mut handler = handler;
        let result = handler.handle(payload).await;
        match result {
            Ok(ok) => Self::write_output(ok, query, conf).await,
            Err(err) => {
                Self::write_error(ZrpcServerError::AppError(err), query).await
            }
        }
        handler
    }

    async fn write_output(out: C::Out, query: Query, conf: &ServerConfig) {
        match C::OutSerde::to_zbyte(&out) {
            Ok(byte) => {
                let reply_key = query.key_expr();
                if let Err(e) = query
                    .reply(reply_key, byte)
                    .congestion_control(conf.reply_congestion)
                    .priority(conf.reply_priority)
                    .await
                {
                    warn!(
                        "RPC server: error on replying '{}', {}",
                        query.key_expr(),
                        e
                    );
                }
            }
            Err(err) => {
                let zse = ZrpcSystemError::EncodeError(AnyError::new(&err));
                Self::write_error(ZrpcServerError::SystemError(zse), query)
                    .await;
            }
        }
    }

    async fn write_error(err: ZrpcServerError<C::Err>, query: Query) {
        let wrapper = C::wrap(err);
        let bytes = C::ErrSerde::to_zbyte(&wrapper)
            .expect("Encode error message error");
        if let Err(e) = query.reply_err(bytes).await {
            warn!(
                "RPC server: error on error replying '{}', {}",
                query.key_expr(),
                e
            );
        };
    }

    #[inline]
    pub fn is_serving(&self) -> bool {
        self.queryable.is_some()
    }

    #[inline]
    pub async fn close(&mut self) {
        if let Some(queryable) = self.queryable.take() {
            if let Err(err) = queryable.undeclare().await {
                error!(
                    "RPC server '{}': error on undeclare: {}",
                    self.config.service_id, err
                );
            };
        }
    }
}
