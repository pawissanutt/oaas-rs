use std::{cmp::min, io::Read, path::PathBuf, time::Duration};

use clap::Parser;
use oprc_invoke::serde::encode;
use oprc_zenoh::OprcZenohConfig;
use rand::Rng;
use rlt::{BenchSuite, IterInfo, IterReport, cli::BenchCli};
use tokio::time::Instant;

use oprc_grpc::{
    InvocationRequest, InvocationResponse, ObjMeta, ObjectInvocationRequest,
    ResponseStatus,
};
use tracing::info;
use tracing_subscriber::EnvFilter;
use zenoh::{
    key_expr::KeyExpr, qos::CongestionControl, query::ConsolidationMode,
};

#[derive(Parser, Clone, Debug)]
pub struct Opts {
    /// Name of class.
    pub cls_id: String,
    /// Name of function.
    pub fn_id: String,
    /// Total number of partitions.
    #[arg(default_value_t = 1)]
    pub partition_count: u16,
    /// Payload as file or stdin if `-` is given. Example: `echo "test" | <commands..> -p -`
    #[arg(short, long)]
    pub payload: Option<clap_stdin::FileOrStdin>,
    /// Size of generated payload.
    pub random_payload_size: Option<usize>,
    /// If invoke to object's function
    #[arg(long)]
    pub stateful: bool,
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
    #[arg(long = "peer", default_value = "false")]
    pub peer_mode: bool,
    /// Number of threads to use for the benchmark.
    #[arg(short, long)]
    pub threads: Option<usize>,
    /// Timeout for each request in milliseconds.
    #[arg(long, default_value_t = 5000)]
    pub timeout: usize,
    /// Function that need to be run before the benchmark.
    #[arg(long)]
    pub init_fn: Option<String>,
    /// Function that need to be run before the benchmark.
    #[arg(long)]
    pub init_payload: Option<PathBuf>,
    /// Starting id for object id.
    #[arg(long, default_value_t = 0)]
    pub starting_id: u64,
}

#[derive(Clone)]
struct InvocationBench {
    sessions: Vec<zenoh::Session>,
    value: Vec<u8>,
    init_payload: Vec<u8>,
    opts: Opts,
}

impl InvocationBench {
    pub async fn new(conf: OprcZenohConfig, opts: Opts) -> Self {
        let value: Vec<u8> = if let Some(size) = opts.random_payload_size {
            rand::rng()
                .sample_iter(&rand::distr::Alphanumeric)
                .take(size)
                .map(u8::from)
                .collect()
        } else if let Some(payload) = &opts.payload {
            let mut tmp = Vec::new();
            let mut reader = payload
                .clone()
                .into_reader()
                .expect("Failed to create reader");
            reader
                .read_to_end(&mut tmp)
                .expect("Failed to read payload");
            tmp
        } else {
            vec![]
        };
        let session_count =
            min(opts.session_count, opts.bench_opts.concurrency.get()) as usize;
        let mut sessions = Vec::with_capacity(session_count);
        for i in 0..session_count {
            let s = zenoh::open(conf.create_zenoh())
                .await
                .expect(format!("Failed to open session {}", i).as_str());
            sessions.push(s);
        }

        let init_payload = match &opts.init_payload {
            Some(init_file) => tokio::fs::read(init_file).await.unwrap(),
            None => Vec::new(),
        };

        info!(
            "use the init function's payload: {} bytes",
            init_payload.len()
        );

        Self {
            sessions: sessions,
            value: value,
            init_payload,
            opts,
        }
    }
}

struct State {
    id: u64,
    partition_id: u16,
    session: zenoh::Session,
    prefix: KeyExpr<'static>,
}

#[async_trait::async_trait]
impl BenchSuite for InvocationBench {
    type WorkerState = State;

    async fn state(&self, id: u32) -> anyhow::Result<Self::WorkerState> {
        let s_index = (id % self.opts.session_count as u32) as usize;
        let partition_id = (id % self.opts.partition_count as u32) as u16;
        let session = self.sessions[s_index].clone();

        let prefix = if self.opts.stateful {
            session
                .declare_keyexpr(format!(
                    "oprc/{}/{}",
                    self.opts.cls_id, partition_id
                ))
                .await
                .unwrap()
        } else {
            session
                .declare_keyexpr(format!(
                    "oprc/{}/{}/invokes/{}",
                    self.opts.cls_id, partition_id, self.opts.fn_id
                ))
                .await
                .unwrap()
        };
        let obj_id = self.opts.starting_id + id as u64 * 100000;

        let proxy = oprc_invoke::proxy::ObjectProxy::new(session.clone());

        if let Some(init_fn) = &self.opts.init_fn {
            let resp = proxy
                .invoke_object_fn(
                    &ObjMeta {
                        cls_id: self.opts.cls_id.clone(),
                        partition_id: partition_id as u32,
                        object_id: obj_id,
                        object_id_str: None,
                    },
                    init_fn,
                    self.init_payload.clone(),
                )
                .await
                .map_err(|err| {
                    eprintln!("Failed to invoke init function to partition={}, obj_id={}: {:?}", partition_id, obj_id, err);
                    anyhow::anyhow!("Failed to invoke init function: {:?}", err)
                })?;
            if resp.status != ResponseStatus::Okay as i32 {
                return Err(anyhow::anyhow!(
                    "Failed to invoke init function: {:?}",
                    resp.payload
                ));
            }

            // let handler = session
            //     .get(format!(
            //         "oprc/{}/{}/objects/{}/invokes/{}",
            //         self.opts.cls_id, partition_id, obj_id, init_fn
            //     ))
            //     .payload(self.init_payload.clone())
            //     .consolidation(ConsolidationMode::None)
            //     .congestion_control(CongestionControl::Block)
            //     .target(QueryTarget::BestMatching)
            //     .timeout(Duration::from_millis(self.opts.timeout as u64))
            //     .await;
            // let handler = handler.map_err(|err| {
            //     anyhow::anyhow!(
            //         "Failed to invoke init function {}: {:?}",
            //         init_fn,
            //         err
            //     )
            // })?;
            // let reply = handler.recv_async().await.map_err(|err| {
            //     anyhow::anyhow!(
            //         "Failed to invoke init function {}: {:?}",
            //         init_fn,
            //         err
            //     )
            // })?;
            // let _ = reply.into_result().map_err(|err| {
            //     anyhow::anyhow!(
            //         "Failed to invoke init function {}: {:?}",
            //         init_fn,
            //         err
            //     )
            // })?;
            // println!("create object with id={}", obj_id);
        }

        Ok(State {
            id: obj_id,
            partition_id,
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
        let id = state.id;
        let mut len = self.value.len();

        let (key, payload) = if self.opts.stateful {
            let key = state
                .prefix
                .join(&format!("objects/{}/invokes/{}", id, self.opts.fn_id))
                .unwrap();

            let req = ObjectInvocationRequest {
                cls_id: self.opts.cls_id.clone(),
                partition_id: state.partition_id as u32,
                fn_id: self.opts.fn_id.clone(),
                object_id: id,
                payload: self.value.clone(),
                ..Default::default()
            };
            (key, encode(&req))
        } else {
            let req = InvocationRequest {
                cls_id: self.opts.cls_id.clone(),
                fn_id: self.opts.fn_id.clone(),
                payload: self.value.clone(),
                ..Default::default()
            };
            (state.prefix.clone(), encode(&req))
        };

        let res = {
            let (rx, tx) = flume::bounded(16);

            let get_result = match state
                .session
                .get(key)
                .payload(payload)
                .consolidation(ConsolidationMode::None)
                .congestion_control(CongestionControl::Block)
                .target(zenoh::query::QueryTarget::BestMatching)
                .timeout(Duration::from_millis(self.opts.timeout as u64))
                .callback(move |s| {
                    let _ = rx.send(s);
                })
                .await
            {
                Ok(_) => tx.recv_async().await,
                Err(_) => {
                    let duration = t.elapsed();
                    return Ok(IterReport {
                        duration,
                        status: rlt::Status::client_error(1),
                        bytes: len as u64,
                        items: 0,
                    });
                }
            };
            let resp = match get_result {
                Ok(reply) => match reply.result() {
                    Result::Ok(sample) => {
                        let resp: InvocationResponse =
                            oprc_invoke::serde::decode(sample.payload())
                                .unwrap();
                        resp
                    }
                    Err(err) => {
                        tracing::error!(
                            "Error processing reply: {:?}",
                            err.payload().try_to_string().unwrap()
                        );
                        let duration = t.elapsed();
                        return Ok(IterReport {
                            duration,
                            status: rlt::Status::server_error(1),
                            bytes: len as u64,
                            items: 0,
                        });
                    }
                },
                Err(_err) => {
                    let duration = t.elapsed();
                    return Ok(IterReport {
                        duration,
                        status: rlt::Status::client_error(2),
                        bytes: len as u64,
                        items: 0,
                    });
                }
            };
            Ok(resp)
        };

        let duration = t.elapsed();
        match res {
            Ok(resp) => {
                if resp.status == ResponseStatus::Okay as i32 {
                    len += resp.payload.map(|v| v.len()).unwrap_or(0);
                    let status = rlt::Status::success(200);
                    Ok(IterReport {
                        duration,
                        status,
                        bytes: len as u64,
                        items: 1,
                    })
                } else {
                    tracing::info!(
                        "Error response: {:?}",
                        String::from_utf8_lossy(
                            &resp.payload.unwrap_or(vec![])
                        )
                    );
                    Ok(IterReport {
                        duration,
                        status: rlt::Status::server_error(resp.status as i64),
                        bytes: len as u64,
                        items: 0,
                    })
                }
            }
            Err(err) => Ok(IterReport {
                duration,
                status: tools::to_status(&err),
                bytes: len as u64,
                items: 0,
            }),
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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("off")),
        )
        .init();
    let opts: Opts = Opts::parse();
    info!("use {opts:?}");
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
            buffer_size: Some(16777216),
            mode,
            gossip_enabled: Some(true),
            ..Default::default()
        };
        let bench = InvocationBench::new(oprc_zenoh, opts.clone()).await;
        rlt::cli::run(opts.bench_opts, bench).await
    });
}
