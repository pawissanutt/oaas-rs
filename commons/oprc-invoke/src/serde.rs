use prost::{DecodeError, Message};
use zenoh::bytes::ZBytes;

#[inline]
pub fn decode<M>(b: &ZBytes) -> Result<M, DecodeError>
where
    M: Message + Default,
{
    M::decode(b.to_bytes().as_ref())
}

#[inline]
pub fn encode<M>(m: &M) -> ZBytes
where
    M: Message,
{
    ZBytes::from(M::encode_to_vec(&m))
}
