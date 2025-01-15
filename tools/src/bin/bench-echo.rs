use anyhow::Ok;
use clap::Parser;
use rand::Rng;
use rlt::{
    cli::BenchCli,
    IterReport, {BenchSuite, IterInfo},
};
use tokio::time::Instant;
use tonic::transport::{Channel, Uri};

use oprc_pb::{oprc_function_client::OprcFunctionClient, InvocationRequest};

#[derive(Parser, Clone)]
pub struct Opts {
    /// Target URL.
    pub url: Uri,
    /// Payload size.
    #[arg(default_value_t = 512)]
    pub size: usize,

    /// Embed BenchCli into this Opts.
    #[command(flatten)]
    pub bench_opts: BenchCli,
}

#[derive(Clone)]
struct HttpBench {
    url: Uri,
    value: bytes::Bytes,
}

impl HttpBench {
    pub fn new(url: Uri, size: usize) -> Self {
        let value: Vec<u8> = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(size)
            .map(u8::from)
            .collect();
        Self {
            url,
            value: value.into(),
        }
    }
}

#[async_trait::async_trait]
impl BenchSuite for HttpBench {
    type WorkerState = OprcFunctionClient<Channel>;

    async fn state(&self, _: u32) -> anyhow::Result<Self::WorkerState> {
        let client = OprcFunctionClient::connect(self.url.clone()).await?;
        Ok(client)
    }

    async fn bench(
        &mut self,
        client: &mut Self::WorkerState,
        _: &IterInfo,
    ) -> anyhow::Result<IterReport> {
        let t = Instant::now();
        let resp = client
            .invoke_fn(InvocationRequest {
                cls_id: "example".into(),
                fn_id: "echo".into(),
                payload: self.value.to_vec(),
                ..Default::default()
            })
            .await;
        let duration = t.elapsed();
        match resp {
            Result::Ok(inner) => {
                let resp = inner.into_inner();
                let bytes = resp.payload.map(|b| b.len()).unwrap_or(0);
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();
    let bench = HttpBench::new(opts.url, opts.size);
    rlt::cli::run(opts.bench_opts, bench).await
}
