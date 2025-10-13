use oprc_grpc::{InvocationRequest, ObjectInvocationRequest};
use tracing::warn;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(default)]
pub struct LoggingReq {
    pub mode: Mode,
    pub inteval: u64,
    pub num: u64,
    pub duration: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Default, Debug, Clone)]
#[serde(default)]
pub struct LoggingResp {
    pub log: Vec<(u64, u64)>,
    pub num: u64,
    pub write_latency: u32,
    pub ts: u64,
}

#[derive(serde::Serialize, serde::Deserialize, Default, Debug, Clone)]
pub enum Mode {
    READ,
    #[default]
    WRITE,
}

impl Default for LoggingReq {
    fn default() -> Self {
        Self {
            mode: Mode::WRITE,
            inteval: 100,
            duration: 30000,
            num: 0,
        }
    }
}

impl TryFrom<&InvocationRequest> for LoggingReq {
    type Error = tonic::Status;
    fn try_from(value: &InvocationRequest) -> Result<Self, Self::Error> {
        if value.payload.is_empty() {
            Ok(LoggingReq::default())
        } else {
            serde_json::from_slice(&value.payload).map_err(|e| {
                warn!("failed to parse func req: {}", e);
                tonic::Status::internal(e.to_string())
            })
        }
    }
}

impl TryFrom<&ObjectInvocationRequest> for LoggingReq {
    type Error = tonic::Status;
    fn try_from(value: &ObjectInvocationRequest) -> Result<Self, Self::Error> {
        if value.payload.is_empty() {
            Ok(LoggingReq::default())
        } else {
            serde_json::from_slice(&value.payload).map_err(|e| {
                warn!("failed to parse func req: {}", e);
                tonic::Status::internal(e.to_string())
            })
        }
    }
}
