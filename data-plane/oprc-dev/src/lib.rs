pub mod num_log;

use std::collections::BTreeMap;
use std::collections::HashMap;

use envconfig::Envconfig;
use oprc_grpc::InvocationRequest;
use oprc_grpc::ObjData;
use oprc_grpc::ObjMeta;
use oprc_grpc::ObjectInvocationRequest;
use oprc_grpc::ValData;
use oprc_grpc::ValType;
use rand::{Rng, distr::Alphanumeric};

#[derive(envconfig::Envconfig)]
pub struct Config {
    #[envconfig(from = "HTTP_PORT", default = "8080")]
    pub http_port: u16,
    #[envconfig(from = "OPRC_ENV", default = "undefine")]
    pub env: String,
    #[envconfig(from = "OPRC_ENV_ID", default = "0")]
    pub env_id: u32,
}

pub fn create_reflection() -> (
    tonic_reflection::server::v1alpha::ServerReflectionServer<
        impl tonic_reflection::server::v1alpha::ServerReflection,
    >,
    tonic_reflection::server::v1::ServerReflectionServer<
        impl tonic_reflection::server::v1::ServerReflection,
    >,
) {
    let reflection_server_v1a = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_grpc::FILE_DESCRIPTOR_SET)
        .build_v1alpha()
        .unwrap();

    let reflection_server_v1 = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(oprc_grpc::FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();
    (reflection_server_v1a, reflection_server_v1)
}

pub fn generate_partition_id() -> u32 {
    let partition_range = std::env::var("OPRC_PARTITION_RANGE")
        .unwrap_or_else(|_| "0-0".to_string());
    let parts: Vec<&str> = partition_range.split('-').collect();
    if parts.len() != 2 {
        panic!(
            "OPRC_PARTITION_RANGE must be in format lower-upper, got {}",
            partition_range
        );
    }
    let lower: u32 = parts[0]
        .parse()
        .expect("Lower bound of OPRC_PARTITION_RANGE must be a valid u32");
    let upper: u32 = parts[1]
        .parse()
        .expect("Upper bound of OPRC_PARTITION_RANGE must be a valid u32");
    if lower > upper {
        panic!(
            "Invalid range: lower ({}) cannot be greater than upper ({})",
            lower, upper
        );
    }
    let partition_id: u32 =
        rand::Rng::random_range(&mut rand::rng(), lower..=upper);
    partition_id
}

fn rand_string(size: u32) -> Vec<u8> {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(size as usize)
        .collect()
}

pub fn rand_json(req: &FuncReq) -> Result<Vec<u8>, serde_json::Error> {
    let mut map = BTreeMap::new();
    for _ in 0..req.entry {
        let key = String::from_utf8(rand_string(req.key_size)).unwrap();
        let value = String::from_utf8(rand_string(req.value_size)).unwrap();
        map.insert(key, value);
    }
    return serde_json::to_vec(&map);
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
#[serde(default)]
pub struct FuncReq {
    pub entry: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub resp_json: bool,
}

impl Default for FuncReq {
    fn default() -> Self {
        Self {
            entry: 10,
            key_size: 10,
            value_size: 10,
            resp_json: true,
        }
    }
}

impl TryFrom<&InvocationRequest> for FuncReq {
    type Error = tonic::Status;
    fn try_from(value: &InvocationRequest) -> Result<Self, Self::Error> {
        if value.payload.is_empty() {
            Ok(FuncReq::default())
        } else {
            serde_json::from_slice(&value.payload).map_err(|e| {
                tracing::warn!("failed to parse func req: {}", e);
                tonic::Status::internal(e.to_string())
            })
        }
    }
}

impl TryFrom<&ObjectInvocationRequest> for FuncReq {
    type Error = tonic::Status;
    fn try_from(value: &ObjectInvocationRequest) -> Result<Self, Self::Error> {
        if value.payload.is_empty() {
            Ok(FuncReq::default())
        } else {
            serde_json::from_slice(&value.payload).map_err(|e| {
                tracing::warn!("failed to parse func req: {}", e);
                tonic::Status::internal(e.to_string())
            })
        }
    }
}

pub fn rand_obj(
    req: &FuncReq,
    meta: ObjMeta,
) -> Result<ObjData, serde_json::Error> {
    let mut entries = HashMap::new();
    entries.insert(
        "0".to_string(),
        ValData {
            data: rand_json(req)?.into(),
            r#type: ValType::Byte as i32,
        },
    );
    Ok(ObjData {
        metadata: Some(meta),
        entries,
        event: None,
    })
}
