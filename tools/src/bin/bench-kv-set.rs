use clap::Parser;
use oprc_zenoh::OprcZenohConfig;
use prost::Message;
use rand::Rng;
use rlt::{
    cli::BenchCli,
    IterReport, {BenchSuite, IterInfo},
};
use std::{collections::HashMap, time::Duration};
use tokio::time::Instant;

use oprc_pb::ObjData;
use zenoh::{key_expr::KeyExpr, query::ConsolidationMode};

#[derive(Parser, Clone)]
pub struct Opts {
    /// Name of collection.
    pub collection: String,
    /// Total number of partitions.
    #[arg(default_value_t = 1)]
    pub partition_count: u16,
    /// Size of generated object data.
    #[arg(default_value_t = 512)]
    pub size: usize,
    /// Embed BenchCli into this Opts.
    #[command(flatten)]
    pub bench_opts: BenchCli,
    /// Zenoh peer to connect to.
    #[arg(short, name = "z", long)]
    pub zenoh_peer: Option<String>,
    /// Zenoh session to be used.
    #[arg(short, long, default_value = "1")]
    pub session_count: u32,
    /// If run Zenoh in peer mode.
    #[arg(short, long = "peer", default_value = "false")]
    pub peer_mode: bool,
    #[clap(short, long)]
    pub threads: Option<usize>,
}

#[derive(Clone)]
struct KvSetBench {
    sessions: Vec<zenoh::Session>,
    value: Vec<u8>,
    opts: Opts,
}

impl KvSetBench {
    pub async fn new(conf: OprcZenohConfig, opts: Opts) -> Self {
        let value: Vec<u8> = rand::rng()
            .sample_iter(&rand::distr::Alphanumeric)
            .take(opts.size)
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
            value: value,
            opts,
        }
    }
}

struct State {
    session: zenoh::Session,
    prefix: KeyExpr<'static>,
    id: u64,
}

#[async_trait::async_trait]
impl BenchSuite for KvSetBench {
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
            id: id as u64 * 100000,
        })
    }

    async fn bench(
        &mut self,
        state: &mut Self::WorkerState,
        _: &IterInfo,
    ) -> anyhow::Result<IterReport> {
        let t = Instant::now();
        let id = state.id;
        state.id += 1;
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
            .target(zenoh::query::QueryTarget::BestMatching)
            .await
        {
            Ok(result) => result.recv_async().await,
            Err(_err) => {
                let duration = t.elapsed();
                return Ok(IterReport {
                    duration,
                    status: rlt::Status::client_error(1),
                    bytes: len as u64,
                    items: 0,
                });
            }
        };

        match get_result {
            Ok(reply) => {
                let duration = t.elapsed();
                match reply.result() {
                    Result::Ok(_) => {
                        let status = rlt::Status::success(200);
                        Ok(IterReport {
                            duration,
                            status,
                            bytes: len as u64,
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
                            bytes: len as u64,
                            items: 0,
                        })
                    }
                }
            }
            Err(_err) => {
                let duration = t.elapsed();
                return Ok(IterReport {
                    duration,
                    status: rlt::Status::client_error(2),
                    bytes: len as u64,
                    items: 0,
                });
            }
        }
    }

    async fn teardown(
        self,
        state: Self::WorkerState,
        _info: IterInfo,
    ) -> anyhow::Result<()> {
        let _ = state.session.close().timeout(Duration::from_secs(0)).await;
        Ok(())
    }
}

fn main() {
    let opts: Opts = Opts::parse();
    let rt = tools::setup_runtime(opts.threads);
    let _ = rt.block_on(async {
        let mode = if opts.peer_mode {
            zenoh_config::WhatAmI::Peer
        } else {
            zenoh_config::WhatAmI::Client
        };
        let oprc_zenoh = OprcZenohConfig {
            peers: opts.zenoh_peer.clone(),
            zenoh_port: 0,
            mode,
            gossip_enabled: Some(true),
            ..Default::default()
        };
        let bench = KvSetBench::new(oprc_zenoh, opts.clone()).await;
        rlt::cli::run(opts.bench_opts, bench).await
    });
}
