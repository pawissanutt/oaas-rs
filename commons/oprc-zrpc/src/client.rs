use zenoh::{
    key_expr::KeyExpr,
    qos::{CongestionControl, Priority},
    query::{ConsolidationMode, QueryTarget},
};

use crate::{ZrpcError, msg::MsgSerde};
use std::marker::PhantomData;

use super::ZrpcTypeConfig;

#[derive(Clone, Debug)]
pub struct ZrpcClientConfig {
    pub service_id: String,
    pub target: QueryTarget,
    pub channel_size: usize,
    pub congestion_control: CongestionControl,
    pub priority: Priority,
}

impl Default for ZrpcClientConfig {
    fn default() -> Self {
        Self {
            service_id: Default::default(),
            target: QueryTarget::BestMatching,
            channel_size: 1,
            congestion_control: CongestionControl::Block,
            priority: Priority::default(),
        }
    }
}

#[derive(Clone)]
pub struct ZrpcClient<C>
where
    C: ZrpcTypeConfig,
{
    key_expr: KeyExpr<'static>,
    z_session: zenoh::Session,
    config: ZrpcClientConfig,
    _conf: PhantomData<C>,
}

impl<C> ZrpcClient<C>
where
    C: ZrpcTypeConfig,
{
    pub async fn new(service_id: String, z_session: zenoh::Session) -> Self {
        let key_expr = z_session
            .declare_keyexpr(service_id.clone())
            .await
            .expect("Declare key_expr for zenoh");
        Self {
            key_expr,
            z_session,
            config: ZrpcClientConfig {
                service_id,
                target: QueryTarget::BestMatching,
                channel_size: 1,
                congestion_control: CongestionControl::Block,
                priority: Priority::default(),
            },
            _conf: PhantomData,
        }
    }

    pub async fn with_config(
        config: ZrpcClientConfig,
        z_session: zenoh::Session,
    ) -> Self {
        let key_expr = z_session
            .declare_keyexpr(config.service_id.clone())
            .await
            .expect("Declare key_expr for zenoh");
        Self {
            key_expr,
            z_session,
            config,
            _conf: PhantomData,
        }
    }

    pub async fn call(
        &self,
        payload: &C::In,
    ) -> Result<C::Out, ZrpcError<C::Err>> {
        let byte = C::InSerde::to_zbyte(&payload)
            .map_err(|e| ZrpcError::EncodeError(e))?;

        let (tx, rx) = flume::bounded(self.config.channel_size);

        let _ = self
            .z_session
            .get(self.key_expr.clone())
            .payload(byte)
            .target(self.config.target)
            .consolidation(ConsolidationMode::None)
            .congestion_control(self.config.congestion_control)
            .priority(self.config.priority)
            // .with((tx, rx))
            .callback(move |s| {
                let _ = tx.send(s);
            })
            .await?;
        let reply = rx.recv_async().await?;
        match reply.result() {
            Ok(sample) => {
                let res = C::OutSerde::from_zbyte(sample.payload())
                    .map_err(|e| ZrpcError::DecodeError(e))?;
                Ok(res)
            }
            Err(err) => {
                let wrapper = C::ErrSerde::from_zbyte(err.payload())
                    .map_err(|e| ZrpcError::DecodeError(e))?;
                let zrpc_server_error = C::unwrap(wrapper);
                let err = match zrpc_server_error {
                    super::ZrpcServerError::AppError(app_err) => {
                        ZrpcError::AppError(app_err)
                    }
                    super::ZrpcServerError::SystemError(zrpc_system_error) => {
                        ZrpcError::ServerSystemError(zrpc_system_error)
                    }
                };
                Err(err)
            }
        }
    }

    pub async fn call_with_key(
        &self,
        key: String,
        payload: &C::In,
    ) -> Result<C::Out, ZrpcError<C::Err>> {
        let byte = C::InSerde::to_zbyte(&payload)
            .map_err(|e| ZrpcError::EncodeError(e))?;

        let (tx, rx) = flume::bounded(self.config.channel_size);

        self.z_session
            .get(self.key_expr.join(&key).unwrap())
            .payload(byte)
            .target(self.config.target)
            .consolidation(ConsolidationMode::None)
            .congestion_control(self.config.congestion_control)
            .priority(self.config.priority)
            .callback(move |s| {
                let _ = tx.send(s);
            })
            .await?;

        let reply = rx.recv_async().await?;
        match reply.result() {
            Ok(sample) => {
                let res = C::OutSerde::from_zbyte(sample.payload())
                    .map_err(|e| ZrpcError::DecodeError(e))?;
                Ok(res)
            }
            Err(err) => {
                let wrapper = C::ErrSerde::from_zbyte(err.payload())
                    .map_err(|e| ZrpcError::DecodeError(e))?;
                let zrpc_server_error = C::unwrap(wrapper);
                let err = match zrpc_server_error {
                    super::ZrpcServerError::AppError(app_err) => {
                        ZrpcError::AppError(app_err)
                    }
                    super::ZrpcServerError::SystemError(zrpc_system_error) => {
                        ZrpcError::ServerSystemError(zrpc_system_error)
                    }
                };
                Err(err)
            }
        }
    }
}
