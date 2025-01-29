use anyhow::Ok;
use clap::Parser;
use rlt::{
    cli::BenchCli,
    IterReport, {BenchSuite, IterInfo},
};
use tokio::time::Instant;

#[derive(Parser, Clone)]
pub struct Opts {
    /// Number of threads to use for the benchmark.
    #[clap(short, long)]
    pub threads: Option<usize>,
    /// Embed BenchCli into this Opts.
    #[command(flatten)]
    pub bench_opts: BenchCli,
}

#[derive(Clone)]
struct NoOptBench {}

impl NoOptBench {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl BenchSuite for NoOptBench {
    type WorkerState = ();

    async fn state(&self, _: u32) -> anyhow::Result<Self::WorkerState> {
        Ok(())
    }

    async fn bench(
        &mut self,
        _client: &mut Self::WorkerState,
        _: &IterInfo,
    ) -> anyhow::Result<IterReport> {
        let t = Instant::now();
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        Ok(IterReport {
            duration: t.elapsed(),
            status: rlt::Status::success(200),
            bytes: 1,
            items: 1,
        })
    }
}

fn main() {
    let opts: Opts = Opts::parse();
    let rt = tools::setup_runtime(opts.threads);
    let _ = rt.block_on(async {
        let bench = NoOptBench::new();
        rlt::cli::run(opts.bench_opts, bench).await
    });
}
