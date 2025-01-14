use std::collections::HashMap;

use bytes::Bytes;
use clap::Parser;
use envconfig::Envconfig;
use oprc_zenoh::OprcZenohConfig;
use prost::Message;
use rand::Rng;
use rlt::{
    cli::BenchCli,
    IterReport, {BenchSuite, IterInfo},
};
use tokio::time::Instant;

use oprc_pb::ObjData;
use zenoh::{key_expr::KeyExpr, query::ConsolidationMode};

#[derive(Parser, Clone)]
pub struct Opts {
    /// Target URL.
    pub collection: String,
    #[arg(default_value_t = 1)]
    pub partition_count: u16,
    /// Target URL.
    #[arg(default_value_t = 512)]
    pub size: usize,

    /// Embed BenchCli into this Opts.
    #[command(flatten)]
    pub bench_opts: BenchCli,

    #[arg(short, name = "z", long)]
    pub zenoh_peer: Option<String>,

    #[arg(short, long, default_value = "1")]
    pub session_count: u32,
}

#[derive(Clone)]
struct HttpBench {
    sessions: Vec<zenoh::Session>,
    value: bytes::Bytes,
    opts: Opts,
}

impl HttpBench {
    pub async fn new(conf: OprcZenohConfig, size: usize, opts: Opts) -> Self {
        let value: Vec<u8> = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(size)
            .map(u8::from)
            .collect();
        let mut sessions = vec![];
        for i in 0..opts.session_count {
            let s = zenoh::open(conf.create_zenoh())
                .await
                .expect(format!("Failed to open session {}", i).as_str());
            sessions.push(s);
        }

        Self {
            sessions: sessions,
            value: Bytes::from(value),
            opts,
        }
    }
}

struct State {
    session: zenoh::Session,
    prefix: KeyExpr<'static>,
}

#[async_trait::async_trait]
impl BenchSuite for HttpBench {
    type WorkerState = State;

    async fn state(&self, id: u32) -> anyhow::Result<Self::WorkerState> {
        let s_index = (id % self.opts.session_count as u32) as usize;
        let partitiion_id = (id % self.opts.partition_count as u32) as u16;
        let session = self.sessions[s_index].clone();
        let prefix = session
            .declare_keyexpr(format!(
                "oprc/{}/{}",
                self.opts.collection, partitiion_id
            ))
            .await
            .unwrap();
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
        let key = state.prefix.join(&format!("objects/{id}/set")).unwrap();
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
        let len = payload.len();
        let get_result = match state
            .session
            .get(key)
            .payload(payload)
            .consolidation(ConsolidationMode::None)
            .target(zenoh::query::QueryTarget::All)
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
                        let bytes = len;
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
    // tracing_subscriber::fmt()
    // .with_max_level(tracing::Level::DEBUG)
    // .init();
    let opts: Opts = Opts::parse();
    let oprc_zenoh = OprcZenohConfig {
        peers: opts.zenoh_peer.clone(),
        zenoh_port: 0,
        mode: zenoh_config::WhatAmI::Client,
        ..Default::default()
    };
    let bench = HttpBench::new(oprc_zenoh, opts.size, opts.clone()).await;
    rlt::cli::run(opts.bench_opts, bench).await
}
