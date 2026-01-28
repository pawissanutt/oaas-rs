use std::marker::PhantomData;

use crate::{MsgSerde, ZrpcTypeConfig, error::ZrpcServerError};
use anyerror::AnyError;
use zenoh::bytes::ZBytes;

pub struct PostcardZrpcType<I, O, E>
where
    I: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    E: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
{
    _i: PhantomData<I>,
    _o: PhantomData<O>,
    _e: PhantomData<E>,
}

impl<I, O, E> ZrpcTypeConfig for PostcardZrpcType<I, O, E>
where
    I: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    O: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
    E: Send + Sync + serde::Serialize + serde::de::DeserializeOwned,
{
    type In = I;
    type Out = O;
    type Err = E;
    type Wrapper = ZrpcServerError<E>;
    type InSerde = PostcardMsgSerde<I>;
    type OutSerde = PostcardMsgSerde<O>;
    type ErrSerde = PostcardMsgSerde<Self::Wrapper>;

    #[inline]
    fn wrap(output: ZrpcServerError<Self::Err>) -> Self::Wrapper {
        output
    }

    #[inline]
    fn unwrap(wrapper: Self::Wrapper) -> ZrpcServerError<Self::Err> {
        wrapper
    }
}

#[derive(Clone)]
pub struct PostcardMsgSerde<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    _data: PhantomData<T>,
}

impl<T> MsgSerde for PostcardMsgSerde<T>
where
    T: serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
{
    type Data = T;

    #[inline]
    fn to_zbyte(payload: &Self::Data) -> Result<ZBytes, AnyError> {
        let payload =
            postcard::to_allocvec(payload).map_err(|e| AnyError::new(&e))?;
        Ok(ZBytes::from(&payload[..]))
    }

    #[inline]
    fn from_zbyte(payload: &ZBytes) -> Result<Self::Data, AnyError> {
        let resp = postcard::from_bytes(&payload.to_bytes())
            .map_err(|e| AnyError::new(&e))?;
        Ok(resp)
    }
}
