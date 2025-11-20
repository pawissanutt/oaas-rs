use std::marker::PhantomData;

use crate::{error::ZrpcServerError, MsgSerde, ZrpcTypeConfig};
use anyerror::AnyError;
use zenoh::bytes::ZBytes;

pub struct BincodeZrpcType<I, O, E>
where
    I: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    E: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
{
    _i: PhantomData<I>,
    _o: PhantomData<O>,
    _e: PhantomData<E>,
}

impl<I, O, E> ZrpcTypeConfig for BincodeZrpcType<I, O, E>
where
    I: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    E: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
{
    type In = I;
    type Out = O;
    type Err = E;
    type Wrapper = ZrpcServerError<E>;
    type InSerde = BincodeMsgSerde<I>;
    type OutSerde = BincodeMsgSerde<O>;
    type ErrSerde = BincodeMsgSerde<Self::Wrapper>;

    #[inline]
    fn wrap(output: ZrpcServerError<Self::Err>) -> Self::Wrapper {
        output
    }

    #[inline]
    fn unwrap(wrapper: Self::Wrapper) -> ZrpcServerError<Self::Err> {
        wrapper
    }
}

const BINCODE_CONFIG: bincode::config::Configuration =
    bincode::config::standard();

#[derive(Clone)]
pub struct BincodeMsgSerde<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    _data: PhantomData<T>,
}

impl<T> MsgSerde for BincodeMsgSerde<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    type Data = T;

    #[inline]
    fn to_zbyte(payload: &Self::Data) -> Result<ZBytes, AnyError> {
        let payload = bincode::serde::encode_to_vec(&payload, BINCODE_CONFIG)
            .map_err(|e| AnyError::new(&e))?;
        Ok(ZBytes::from(&payload[..]))
    }

    #[inline]
    fn from_zbyte(payload: &ZBytes) -> Result<Self::Data, AnyError> {
        let resp = bincode::serde::decode_from_slice(
            &payload.to_bytes(),
            BINCODE_CONFIG,
        )
        .map_err(|e| AnyError::new(&e))?
        .0;
        return Ok(resp);
    }
}
