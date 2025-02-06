use std::time::{Duration, Instant};

use clap::Parser;
use envconfig::Envconfig;
use oprc_dev::num_log::{LoggingReq, LoggingResp, Mode::WRITE};
use oprc_offload::proxy::ObjectProxy;
use oprc_pb::ObjMeta;
use tracing::{debug, warn};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // log init with default is off
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "off".to_string()),
        ))
        .init();
    debug!("Starting check-deplay program");
    let conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let z = conf.create_zenoh();
    let z_session = zenoh::open(z).await.unwrap();
    let cmd = CheckDelayCommands::parse();
    let mut partition_id = 0;
    let mut handles = Vec::with_capacity(cmd.concurrency as usize);
    for i in 0..cmd.concurrency {
        let runner = Runner::new(
            &cmd,
            &ObjectProxy::new(z_session.clone()),
            partition_id,
            i as u64,
        );
        let h = tokio::spawn(async move { runner.run_loop().await });
        handles.push(h);
        partition_id = (partition_id + 1) % cmd.partition_count as u32;
    }

    let mut all_delays = Vec::new();
    for h in handles {
        let delays = h.await??;
        all_delays.push(delays);
    }
    debug!("All runners completed, collected delays: {:?}", all_delays);

    let mean = all_delays.iter().flatten().sum::<u32>() as f32
        / all_delays.iter().flatten().count() as f32;
    let max = all_delays.iter().flatten().max().unwrap_or(&0);
    let min = all_delays.iter().flatten().min().unwrap_or(&0);
    let p99 = {
        let mut v: Vec<u32> = all_delays.iter().flatten().copied().collect();
        let idx = (0.99 * v.len() as f32) as usize;
        v.select_nth_unstable_by(idx, |a, b| a.cmp(b));
        v[idx]
    };
    let summary = serde_json::json!({
        "mean": mean,
        "max": max,
        "min": min,
        "p99": p99,
        "concurrency": cmd.concurrency
    });
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    debug!("Zenoh session opened successfully");
    Ok(())
}

#[derive(Clone)]
struct Runner {
    cmd: CheckDelayCommands,
    proxy: ObjectProxy,
    obj_meta: ObjMeta,
}

impl Runner {
    pub fn new(
        cmd: &CheckDelayCommands,
        proxy: &ObjectProxy,
        partition_id: u32,
        object_id: u64,
    ) -> Self {
        let obj_meta = ObjMeta {
            cls_id: cmd.cls_id.clone(),
            partition_id,
            object_id,
        };
        Self {
            cmd: cmd.clone(),
            proxy: proxy.clone(),
            obj_meta,
        }
    }

    async fn run_loop(&self) -> anyhow::Result<Vec<u32>> {
        let runner = self.clone();
        let handle = tokio::spawn(async move { runner.call_read().await });

        let start = Instant::now();
        let mut log = Vec::new();
        let mut last_num = 0;
        // Use configurable duration and interval
        let duration = self.cmd.duration_ms;
        let interval = self.cmd.interval_ms;

        while start.elapsed().as_millis() < duration as u128 {
            debug!(
                "Iteration started, elapsed {} ms",
                start.elapsed().as_millis()
            );
            let start_i = Instant::now();
            last_num += 1;
            let resp = self.call_write(last_num).await?;
            debug!(
                "Write call completed for num {} with response: {:?}",
                last_num, resp
            );
            log.push((resp.num, resp.ts));
            let sleep_time =
                interval.saturating_sub(start_i.elapsed().as_millis() as u64);
            if sleep_time > 0 {
                tokio::time::sleep(Duration::from_millis(sleep_time)).await;
            }
        }

        let resp = handle.await.unwrap()?;
        Ok(compare(log, resp.log))
    }

    async fn call_write(&self, num: u64) -> anyhow::Result<LoggingResp> {
        let req = LoggingReq {
            mode: WRITE,
            num,
            ..Default::default()
        };
        let resp = self
            .proxy
            .invoke_object_fn(
                &self.obj_meta,
                &self.cmd.write_func,
                serde_json::to_vec(&req)?,
            )
            .await?;
        if let Some(payload) = resp.payload {
            let resp = serde_json::from_slice(&payload)?;
            debug!("call_write: Successfully processed write for num {}", num);
            Ok(resp)
        } else {
            warn!("call_write: no payload in response for num {}", num);
            return Err(anyhow::anyhow!("no payload in response"));
        }
    }

    async fn call_read(&self) -> anyhow::Result<LoggingResp> {
        let req = LoggingReq {
            ..Default::default()
        };
        let resp = self
            .proxy
            .invoke_object_fn(
                &self.obj_meta,
                &self.cmd.read_func,
                serde_json::to_vec(&req)?,
            )
            .await?;
        if let Some(payload) = resp.payload {
            let resp = serde_json::from_slice(&payload)?;
            debug!("call_read: Successfully processed read");
            Ok(resp)
        } else {
            warn!("call_read: no payload in response");
            return Err(anyhow::anyhow!("no payload in response"));
        }
    }
}

/// compare two logs to calculate the delay between write and read
/// the log is a vector of (num, ts). Each num is a unique number for each write.
/// However, the read can log duplicate num
/// return vec of delays
fn compare(write_log: Vec<(u64, u64)>, read_log: Vec<(u64, u64)>) -> Vec<u32> {
    debug!(
        "Comparing logs: {} write entries, {} read entries",
        write_log.len(),
        read_log.len()
    );
    let mut delays = Vec::new();

    // For each write event, find its corresponding read event (the first read with the same num)
    for (w_num, w_ts) in write_log {
        // Find the first read event with the same num and with a timestamp not earlier than the write timestamp.
        if let Some((_, r_ts)) = read_log
            .iter()
            .find(|&&(r_num, r_ts)| r_num == w_num && r_ts >= w_ts)
        {
            let delay = r_ts - w_ts;
            delays.push(delay as u32);
        }
    }
    debug!("Comparison complete, computed delays: {:?}", delays);
    delays
}

#[derive(clap::Parser, Clone, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct CheckDelayCommands {
    /// Name of collection.
    pub cls_id: String,
    /// Total number of partitions.
    #[arg(default_value_t = 1)]
    pub partition_count: u16,

    #[clap(flatten)]
    pub conn: ConnectionArgs,
    /// Read function
    #[arg(short, long, default_value = "log")]
    read_func: String,

    /// Write function
    #[arg(short, long, default_value = "log")]
    write_func: String,

    /// Concurrency level
    #[arg(short, long, default_value_t = 1)]
    pub concurrency: u32,
    /// Loop duration in milliseconds
    #[arg(short, long, default_value_t = 30000)]
    pub duration_ms: u64,
    /// Loop interval in milliseconds
    #[arg(short, long, default_value_t = 1000)]
    pub interval_ms: u64,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ConnectionArgs {
    /// Zenoh peer to connect to if using Zenoh protocol
    #[arg(short = 'z', long, global = true)]
    pub zenoh_peer: Option<String>,
    /// If using zenoh in peer mode
    #[arg(long, default_value = "false", global = true)]
    pub peer: bool,
}
