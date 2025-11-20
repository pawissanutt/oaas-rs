use std::marker::PhantomData;

use anyerror::AnyError;
use zenoh::bytes::ZBytes;

use crate::MsgSerde;

#[derive(Clone)]
pub struct ProstMsgSerde<T>
where
    T: prost::Message + Send + Sync,
{
    _data: PhantomData<T>,
}

impl<T> MsgSerde for ProstMsgSerde<T>
where
    T: prost::Message + Send + Sync + Default,
{
    type Data = T;

    fn to_zbyte(payload: &T) -> Result<ZBytes, AnyError> {
        let mut buf = Vec::new();
        payload.encode(&mut buf).map_err(|e| AnyError::new(&e))?;
        Ok(ZBytes::from(buf))
    }

    fn from_zbyte(payload: &ZBytes) -> Result<T, AnyError> {
        let buf = &payload.to_bytes();
        T::decode(buf.as_ref()).map_err(|e| AnyError::new(&e))
    }
}
