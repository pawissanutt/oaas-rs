use anyerror::AnyError;

use zenoh::bytes::ZBytes;

pub trait MsgSerde: Send + Sync {
    type Data: Send + Sync;

    fn to_zbyte(payload: &Self::Data) -> Result<ZBytes, AnyError>;

    fn from_zbyte(payload: &ZBytes) -> Result<Self::Data, AnyError>;
}
