use std::collections::HashMap;

use bytes::Bytes;
use clap::Parser;
use envconfig::Envconfig;
use prost::Message;
use rand::Rng;
use rlt::{
    cli::BenchCli,
    IterReport, {BenchSuite, IterInfo},
};
use tokio::time::Instant;

use oprc_pb::ObjData;
use zenoh::query::ConsolidationMode;

#[derive(Parser, Clone)]
pub struct Opts {
    /// Target URL.
    pub prefix: String,
    /// Target URL.
    #[arg(default_value_t = 512)]
    pub size: usize,

    /// Embed BenchCli into this Opts.
    #[command(flatten)]
    pub bench_opts: BenchCli,
}

#[derive(Clone)]
struct HttpBench {
    session: zenoh::Session,
    value: bytes::Bytes,
    prefix: String,
}

impl HttpBench {
    pub async fn new(
        z_config: zenoh::Config,
        size: usize,
        prefix: String,
    ) -> Self {
        let value: Vec<u8> = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(size)
            .map(u8::from)
            .collect();
        let session =
            zenoh::open(z_config).await.expect("Failed to open session");

        Self {
            session: session,
            value: Bytes::from(value),
            prefix,
        }
    }
}

struct State {
    session: zenoh::Session,
    prefix: zenoh::key_expr::KeyExpr<'static>,
}

#[async_trait::async_trait]
impl BenchSuite for HttpBench {
    type WorkerState = State;

    async fn state(&self, _: u32) -> anyhow::Result<Self::WorkerState> {
        let session = self.session.clone();
        let prefix =
            session.declare_keyexpr(self.prefix.clone()).await.unwrap();

        Ok(State {
            session: session,
            prefix,
        })
    }

    async fn bench(
        &mut self,
        state: &mut Self::WorkerState,
        _: &IterInfo,
    ) -> anyhow::Result<IterReport> {
        let t = Instant::now();
        let id: u64 = rand::random();
        let key = state.prefix.join(&format!("{id}/set")).unwrap();
        tracing::debug!("key: {:?}\n", key);
        let mut entries = HashMap::new();
        entries.insert(
            0 as u32,
            oprc_pb::ValData {
                data: Some(oprc_pb::val_data::Data::Byte(self.value.clone())),
            },
        );
        let data = ObjData {
            entries: entries,
            ..Default::default()
        };
        let payload = ObjData::encode_to_vec(&data);

        let get_result = match state
            .session
            .get(key)
            .payload(payload)
            .consolidation(ConsolidationMode::None)
            .await
        {
            Ok(result) => result.recv_async().await,
            Err(_err) => {
                let duration = t.elapsed();
                return Ok(IterReport {
                    duration,
                    status: rlt::Status::client_error(1),
                    bytes: 0,
                    items: 1,
                });
            }
        };

        match get_result {
            Ok(reply) => {
                let duration = t.elapsed();
                match reply.result() {
                    Result::Ok(sample) => {
                        let bytes = sample.payload().len();
                        let status = rlt::Status::success(200);
                        Ok(IterReport {
                            duration,
                            status,
                            bytes: bytes as u64,
                            items: 1,
                        })
                    }
                    Err(err) => {
                        tracing::error!(
                            "Error processing reply: {:?}",
                            err.payload().try_to_string().unwrap()
                        );
                        Ok(IterReport {
                            duration,
                            status: rlt::Status::server_error(1),
                            bytes: 0,
                            items: 1,
                        })
                    }
                }
            }
            Err(_err) => {
                let duration = t.elapsed();
                return Ok(IterReport {
                    duration,
                    status: rlt::Status::client_error(2),
                    bytes: 0,
                    items: 1,
                });
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let opts: Opts = Opts::parse();
    let mut oprc_zenoh = oprc_zenoh::OprcZenohConfig::init_from_env()?;
    oprc_zenoh.mode = zenoh_config::WhatAmI::Client;
    let bench =
        HttpBench::new(oprc_zenoh.create_zenoh(), opts.size, opts.prefix).await;
    rlt::cli::run(opts.bench_opts, bench).await
}
