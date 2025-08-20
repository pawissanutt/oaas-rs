use std::time::Duration;

use clap::Parser;
use oprc_zenoh::OprcZenohConfig;
use rlt::{
    IterReport,
    cli::BenchCli,
    {BenchSuite, IterInfo},
};
use tokio::time::Instant;

use zenoh::query::ConsolidationMode;

#[derive(Parser, Clone)]
pub struct Opts {
    /// Name of collection.
    pub collection: String,
    /// Total number of partitions.
    #[arg(default_value_t = 1)]
    pub partition_count: u16,
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

    /// Number of threads to use for the benchmark.
    #[clap(short, long)]
    pub threads: Option<usize>,
}

#[derive(Clone)]
struct HttpBench {
    sessions: Vec<zenoh::Session>,
    opts: Opts,
}

impl HttpBench {
    pub async fn new(conf: OprcZenohConfig, opts: Opts) -> Self {
        let mut sessions = vec![];
        for i in 0..opts.session_count {
            let s = zenoh::open(conf.create_zenoh())
                .await
                .expect(format!("Failed to open session {}", i).as_str());
            sessions.push(s);
        }

        Self {
            sessions: sessions,
            opts,
        }
    }
}

struct State {
    session: zenoh::Session,
    prefix: zenoh::key_expr::KeyExpr<'static>,
    id: u64,
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
        let key = state.prefix.join(&format!("objects/{id}")).unwrap();
        tracing::debug!("key: {:?}\n", key);

        let get_result = match state
            .session
            .get(key)
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
                    bytes: 0,
                    items: 0,
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
                    bytes: 0,
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
        let bench = HttpBench::new(oprc_zenoh, opts.clone()).await;
        rlt::cli::run(opts.bench_opts, bench).await
    });
}
