use std::error::Error;

use bincode::error::EncodeError;
use zenoh::{bytes::ZBytes, query::ReplyError, Session};

use crate::ServiceIdentifier;

#[allow(dead_code)]
pub struct ServiceClient<I: MsgSerde, O: MsgSerde> {
    service_id: ServiceIdentifier,
    z_session: Session,
    req_serde: I,
    resp_serde: O,
}

pub trait MsgSerde {
    type Data: serde::Serialize + serde::de::DeserializeOwned;

    const BINCODE_CONFIG: bincode::config::Configuration =
        bincode::config::standard();

    fn to_zbyte(payload: Self::Data) -> Result<ZBytes, RpcError> {
        let payload =
            bincode::serde::encode_to_vec(payload, Self::BINCODE_CONFIG)?;
        Ok(ZBytes::from(&payload[..]))
    }

    fn from_zbyte(payload: &ZBytes) -> Result<Self::Data, RpcError> {
        let resp = bincode::serde::decode_from_slice(
            &payload.to_bytes(),
            Self::BINCODE_CONFIG,
        )
        .unwrap()
        .0;
        return Ok(resp);
    }
}

#[derive(thiserror::Error, Debug)]
pub enum RpcError {
    #[error("reply error: {0}")]
    ReplyError(#[from] ReplyError),
    #[error("connection error: {0}")]
    ConnectionError(#[from] Box<dyn Error + Send + Sync>),
    #[error("encode error: {0}")]
    EncodeError(#[from] EncodeError),
}

impl<I: MsgSerde, O: MsgSerde> ServiceClient<I, O> {
    pub async fn call(
        &self,
        key: &str,
        payload: I::Data,
    ) -> Result<O::Data, RpcError> {
        let sel = format!(
            "@rpc/oprc/{}/{}/{}/{}",
            self.service_id.class_id,
            self.service_id.partition_id,
            self.service_id.replica_id,
            key,
        );

        let get_result = self
            .z_session
            .get(sel)
            .payload(I::to_zbyte(payload)?)
            .await?;
        let reply = get_result.recv_async().await?;
        let sample = reply.into_result()?;
        O::from_zbyte(sample.payload())
    }
}

#[cfg(test)]
mod test {
    use super::MsgSerde;

    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct TestReq();
    #[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
    struct TestResp();
    struct ReqMessager();

    impl MsgSerde for ReqMessager {
        type Data = TestReq;
    }
    struct RespMessager();
    impl MsgSerde for RespMessager {
        type Data = TestResp;
    }

    #[test]
    fn test_encode() {
        let req = TestReq();
        let _b = ReqMessager::to_zbyte(req).unwrap();
    }
}
