use std::marker::PhantomData;

use crate::{MsgSerde, ZrpcTypeConfig, error::ZrpcServerError};
use anyerror::AnyError;
use zenoh::bytes::ZBytes;

pub struct BitcodeZrpcType<I, O, E>
where
    I: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    E: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
{
    _i: PhantomData<I>,
    _o: PhantomData<O>,
    _e: PhantomData<E>,
}

impl<I, O, E> ZrpcTypeConfig for BitcodeZrpcType<I, O, E>
where
    I: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    E: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
{
    type In = I;
    type Out = O;
    type Err = E;
    type Wrapper = ZrpcServerError<E>;
    type InSerde = BitcodeMsgSerde<I>;
    type OutSerde = BitcodeMsgSerde<O>;
    type ErrSerde = BitcodeMsgSerde<Self::Wrapper>;

    fn wrap(output: ZrpcServerError<Self::Err>) -> Self::Wrapper {
        output
    }

    fn unwrap(wrapper: Self::Wrapper) -> ZrpcServerError<Self::Err> {
        wrapper
    }
}

#[derive(Clone)]
pub struct BitcodeMsgSerde<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    _data: PhantomData<T>,
}

impl<T> MsgSerde for BitcodeMsgSerde<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    type Data = T;

    fn to_zbyte(payload: &Self::Data) -> Result<ZBytes, AnyError> {
        let payload =
            bitcode::serialize(&payload).map_err(|e| AnyError::new(&e))?;
        Ok(ZBytes::from(&payload[..]))
    }

    fn from_zbyte(payload: &ZBytes) -> Result<Self::Data, AnyError> {
        bitcode::deserialize(&payload.to_bytes()).map_err(|e| AnyError::new(&e))
    }
}
