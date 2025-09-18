use std::{io::Read, time::Duration};

use clap::Parser;
use rand::Rng;
use rlt::{BenchSuite, IterInfo, IterReport, cli::BenchCli};
use tokio::time::Instant;

use oprc_grpc::{
    InvocationRequest, ObjectInvocationRequest,
    oprc_function_client::OprcFunctionClient,
};
use tonic::transport::Channel;

#[derive(Parser, Clone)]
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
    /// If use incremental id for object id.
    #[arg(long)]
    pub incremental_id: bool,
    /// If invoke to object's function
    #[arg(long)]
    pub stateful: bool,
    /// Embed BenchCli into this Opts.
    #[command(flatten)]
    pub bench_opts: BenchCli,
    /// Zenoh peer to connect to.
    #[arg(short = 'g', long)]
    pub grpc_url: String,
    /// Number of threads to use for the benchmark.
    #[clap(short, long)]
    pub threads: Option<usize>,
    /// Timeout for each request in milliseconds.
    #[arg(long, default_value_t = 5000)]
    pub timeout: u64,

    #[arg(short, long, default_value = "32")]
    pub max_channel: u32,
}

#[derive(Clone)]
struct InvocationBench {
    value: Vec<u8>,
    opts: Opts,
    pool: Vec<Channel>,
}

impl InvocationBench {
    pub async fn new(opts: Opts) -> Self {
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

        let channel_size =
            std::cmp::min(opts.max_channel, opts.bench_opts.concurrency.get())
                as usize;
        let mut channels = Vec::with_capacity(channel_size);

        for _ in 0..channel_size {
            let channel = Channel::from_shared(opts.grpc_url.clone())
                .expect("Failed to create channel")
                .timeout(Duration::from_millis(opts.timeout))
                .connect()
                .await
                .expect(&format!("Failed to connect to {}", opts.grpc_url));
            channels.push(channel.clone());
        }

        Self {
            value: value,
            opts,
            pool: channels,
        }
    }
}

struct State {
    id: u64,
    partition_id: u16,
    client: OprcFunctionClient<Channel>,
}

#[async_trait::async_trait]
impl BenchSuite for InvocationBench {
    type WorkerState = State;

    async fn state(&self, id: u32) -> anyhow::Result<Self::WorkerState> {
        let partition_id = (id % self.opts.partition_count as u32) as u16;
        let channel = self
            .pool
            .get(id as usize % self.opts.max_channel as usize)
            .unwrap()
            .clone();

        let client = OprcFunctionClient::new(channel)
            .max_decoding_message_size(usize::MAX)
            .max_encoding_message_size(usize::MAX);

        Ok(State {
            id: id as u64 * 100000,
            partition_id,
            client: client,
        })
    }

    async fn bench(
        &mut self,
        state: &mut Self::WorkerState,
        _: &IterInfo,
    ) -> anyhow::Result<IterReport> {
        let t = Instant::now();
        let resp = if self.opts.stateful {
            let object_id = state.id;
            if self.opts.incremental_id {
                state.id += 1;
            }
            state
                .client
                .invoke_obj(ObjectInvocationRequest {
                    cls_id: self.opts.cls_id.clone(),
                    fn_id: self.opts.fn_id.clone(),
                    partition_id: state.partition_id as u32,
                    object_id,
                    payload: self.value.to_vec().into(),
                    ..Default::default()
                })
                .await
        } else {
            state
                .client
                .invoke_fn(InvocationRequest {
                    cls_id: self.opts.cls_id.clone(),
                    fn_id: self.opts.fn_id.clone(),
                    payload: self.value.to_vec().into(),
                    ..Default::default()
                })
                .await
        };
        let duration = t.elapsed();
        match resp {
            Result::Ok(inner) => {
                let resp = inner.into_inner();
                let bytes = resp.payload.map(|b| b.len()).unwrap_or(0)
                    + self.value.len();
                let status = rlt::Status::success(200);
                return Ok(IterReport {
                    duration,
                    status,
                    bytes: bytes as u64,
                    items: 1,
                });
            }
            Err(s) => {
                return Ok(IterReport {
                    duration,
                    status: rlt::Status::error(s.code() as i64),
                    bytes: 0,
                    items: 1,
                });
            }
        }
    }

    async fn teardown(
        self,
        _state: Self::WorkerState,
        _info: IterInfo,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    let opts: Opts = Opts::parse();
    let rt = tools::setup_runtime(opts.threads);
    let _ = rt.block_on(async {
        let bench = InvocationBench::new(opts.clone()).await;
        rlt::cli::run(opts.bench_opts, bench).await
    });
}
