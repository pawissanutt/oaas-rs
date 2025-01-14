use clap::Parser;
use envconfig::Envconfig;
use oprc_zenoh::OprcZenohConfig;
use rlt::{
    cli::BenchCli,
    IterReport, {BenchSuite, IterInfo},
};
use tokio::time::Instant;

use zenoh::query::ConsolidationMode;

#[derive(Parser, Clone)]
pub struct Opts {
    /// Target URL.
    pub prefix: String,
    /// Embed BenchCli into this Opts.
    #[command(flatten)]
    pub bench_opts: BenchCli,

    #[arg(short, name = "z", long)]
    pub zenoh_peer: Option<String>,
}

#[derive(Clone)]
struct HttpBench {
    session: zenoh::Session,
    prefix: String,
}

impl HttpBench {
    pub async fn new(z_config: zenoh::Config, prefix: String) -> Self {
        let session =
            zenoh::open(z_config).await.expect("Failed to open session");

        Self {
            session: session,
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
        let key = state.prefix.join(&format!("{id}")).unwrap();
        tracing::debug!("key: {:?}\n", key);

        let get_result = match state
            .session
            .get(key)
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
    let oprc_zenoh = OprcZenohConfig {
        peers: opts.zenoh_peer,
        // mode: zenoh_config::WhatAmI::Client,
        ..Default::default()
    };
    let bench = HttpBench::new(oprc_zenoh.create_zenoh(), opts.prefix).await;
    rlt::cli::run(opts.bench_opts, bench).await
}
