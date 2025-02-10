use std::{
    fmt::Debug,
    time::{Duration, Instant},
};

use clap::Parser;
use envconfig::Envconfig;
use oprc_dev::num_log::{
    LoggingReq, LoggingResp,
    Mode::{self, WRITE},
};
use oprc_offload::proxy::ObjectProxy;
use oprc_pb::ObjMeta;
use tracing::{debug, info, warn};
use zenoh::config::WhatAmI;

// NEW: Updated helper function to compute stats including count
fn compute_stats(values: &[u32]) -> (f32, f32, u32, u32, u32, u32) {
    let count = values.len() as u32;
    let mean = if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<u32>() as f32 / count as f32
    };
    let std_dev = if values.is_empty() {
        0.0
    } else {
        let variance = values
            .iter()
            .map(|&val| {
                let diff = val as f32 - mean;
                diff * diff
            })
            .sum::<f32>()
            / count as f32;
        variance.sqrt()
    };
    let max = *values.iter().max().unwrap_or(&0);
    let min = *values.iter().min().unwrap_or(&0);
    let p99 = if values.is_empty() {
        0
    } else {
        let mut v = values.to_vec();
        let idx = (0.99 * v.len() as f32) as usize;
        v.select_nth_unstable_by(idx, |a, b| a.cmp(b));
        v[idx]
    };
    (mean, std_dev, max, min, p99, count)
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // log init with default is off
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "off".to_string()),
        ))
        .init();
    info!("Starting check-deplay program");
    let mut conf = oprc_zenoh::OprcZenohConfig::init_from_env().unwrap();
    let cmd = CheckDelayCommands::parse();
    conf.default_query_timout = Some(cmd.duration_ms * 2);
    conf.peers = cmd.conn.zenoh_peer.clone();
    conf.mode = if cmd.conn.peer_mode {
        WhatAmI::Peer
    } else {
        WhatAmI::Client
    };
    let z = conf.create_zenoh();
    let z_session = zenoh::open(z).await.unwrap();
    let mut partition_id = 0;
    let mut handles = Vec::with_capacity(cmd.concurrency as usize);
    // Change the percentage value to the maximum allowed (100) if it exceeds that value.
    let num_read = cmd.max_read_loop;
    for i in 0..cmd.concurrency {
        let run_read = i < num_read;
        let runner = Runner::new(
            &cmd,
            &ObjectProxy::new(z_session.clone()),
            partition_id,
            i as u64,
            run_read,
        );
        let h = tokio::spawn(async move { runner.run_loop().await });
        handles.push(h);
        partition_id = (partition_id + 1) % cmd.partition_count as u32;
    }

    let mut all_delays = Vec::new();
    let mut all_latency_logs = Vec::new();
    let mut all_invocation_latency_logs = Vec::new(); // NEW: to aggregate invocation latencies
    for h in handles {
        let (delays, latency, invocation_latency) = h.await??; // Updated to include invocation_latency
        all_delays.extend(delays);
        all_latency_logs.extend(latency);
        all_invocation_latency_logs.extend(invocation_latency); // NEW: aggregate invocation latencies
    }
    info!("All runners completed, collected delays: {:?}", all_delays);

    // Use helper function for delay stats
    let (mean, std_dev, max, min, p99, count) = compute_stats(&all_delays);
    // Use helper function for latency stats
    let (
        latency_mean,
        latency_std_dev,
        latency_max,
        latency_min,
        latency_p99,
        latency_count,
    ) = compute_stats(&all_latency_logs);
    // Use helper function for invocation latency stats
    let (
        invocation_latency_mean,
        invocation_latency_std_dev,
        invocation_latency_max,
        invocation_latency_min,
        invocation_latency_p99,
        invocation_latency_count,
    ) = compute_stats(&all_invocation_latency_logs); // NEW: compute invocation latency stats

    let summary = serde_json::json!({
        "delay_mean": mean,
        "delay_std_dev": std_dev,
        "delay_max": max,
        "delay_min": min,
        "delay_p99": p99,
        "delay_count": count,
        "write_latency_mean": latency_mean,          // updated key
        "write_latency_std_dev": latency_std_dev,      // updated key
        "write_latency_max": latency_max,              // updated key
        "write_latency_min": latency_min,              // updated key
        "write_latency_p99": latency_p99,              // updated key
        "write_latency_count": latency_count,          // updated key
        "invocation_latency_mean": invocation_latency_mean, // NEW: include invocation latency stats
        "invocation_latency_std_dev": invocation_latency_std_dev, // NEW: include invocation latency stats
        "invocation_latency_max": invocation_latency_max, // NEW: include invocation latency stats
        "invocation_latency_min": invocation_latency_min, // NEW: include invocation latency stats
        "invocation_latency_p99": invocation_latency_p99, // NEW: include invocation latency stats
        "invocation_latency_count": invocation_latency_count, // NEW: include invocation latency stats
        "concurrency": cmd.concurrency,
        "group": cmd.note,
    });
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    info!("Zenoh session opened successfully");
    Ok(())
}

#[derive(Clone)]
struct Runner {
    cmd: CheckDelayCommands,
    proxy: ObjectProxy,
    obj_meta: ObjMeta,
    run_read: bool,
}

impl Debug for Runner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runner")
            .field("obj_meta", &self.obj_meta)
            .field("run_read", &self.run_read)
            .finish()
    }
}

impl Runner {
    pub fn new(
        cmd: &CheckDelayCommands,
        proxy: &ObjectProxy,
        partition_id: u32,
        object_id: u64,
        run_read: bool,
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
            run_read,
        }
    }

    #[tracing::instrument]
    async fn run_loop(&self) -> anyhow::Result<(Vec<u32>, Vec<u32>, Vec<u32>)> {
        // Updated return type
        let maybe_handle = if self.run_read {
            let runner = self.clone();
            Some(tokio::spawn(async move { runner.call_read().await }))
        } else {
            None
        };

        let start = Instant::now();
        let mut log = Vec::new();
        let mut last_num = 0;
        // Use configurable duration and interval
        let duration = self.cmd.duration_ms;
        let interval = self.cmd.interval_ms;
        let mut latency_log = Vec::new();
        let mut invocation_latency_log = Vec::new(); // NEW: to record invocation latency

        while start.elapsed().as_millis() < duration as u128 {
            debug!(
                "Iteration started, elapsed {} ms",
                start.elapsed().as_millis()
            );
            let start_i = Instant::now();
            last_num += 1;
            let resp = self.call_write(last_num).await?;
            let invocation_latency = start_i.elapsed().as_millis() as u32; // NEW: capture invocation time
            debug!(
                "Write call completed for num {} with response: {:?}, invocation latency: {} ms",
                last_num, resp, invocation_latency
            );
            log.push((resp.num, resp.ts));
            latency_log.push(resp.write_latency);
            invocation_latency_log.push(invocation_latency);
            let sleep_time =
                interval.saturating_sub(start_i.elapsed().as_millis() as u64);
            if sleep_time > 0 {
                tokio::time::sleep(Duration::from_millis(sleep_time)).await;
            }
        }

        if let Some(handle) = maybe_handle {
            let resp = handle.await.unwrap();
            match resp {
                Ok(resp) => {
                    debug!("Read call completed successfully");
                    Ok((
                        compare(log, resp.log),
                        latency_log,
                        invocation_latency_log,
                    )) // Updated to include invocation_latency_log
                }
                Err(e) => {
                    warn!("Read call failed: {:?}", e);
                    Err(anyhow::anyhow!("Read call failed"))
                }
            }
        } else {
            Ok((Vec::new(), latency_log, invocation_latency_log)) // Updated to include invocation_latency_log
        }
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
            info!("call_write: Successfully processed write for num {}", num);
            Ok(resp)
        } else {
            warn!("call_write: no payload in response for num {}", num);
            return Err(anyhow::anyhow!("no payload in response"));
        }
    }

    async fn call_read(&self) -> anyhow::Result<LoggingResp> {
        let req = LoggingReq {
            mode: Mode::READ,
            inteval: self.cmd.read_interval_ms,
            duration: self.cmd.duration_ms + self.cmd.read_extend_duration_ms,
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
            let resp = serde_json::from_slice(&payload);
            match resp {
                Ok(resp) => {
                    debug!("call_read: Successfully processed read");
                    Ok(resp)
                }
                Err(e) => {
                    warn!("call_read: Failed to deserialize response payload: {:?}", e);
                    Err(anyhow::anyhow!(
                        "Failed to deserialize response payload"
                    ))
                }
            }
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
    debug!("Write log: {:?}", write_log);
    debug!("Read log: {:?}", read_log);
    let mut delays = Vec::new();

    for (num, ts) in write_log.iter() {
        let latest_read = read_log
            .iter()
            .filter(|(n, rts)| *n < *num && rts > ts)
            .map(|(_, rts)| (rts - ts) as u32)
            .max();
        if let Some(delay) = latest_read {
            delays.push(delay);
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
    /// Note for the run
    pub note: Option<String>,

    #[clap(flatten)]
    pub conn: ConnectionArgs,
    /// Read function
    #[arg(short, long, default_value = "log2")]
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
    #[arg(long, default_value_t = 100)]
    pub max_read_loop: u32,
    #[arg(long, default_value_t = 100)]
    pub read_interval_ms: u64,
    #[arg(long, default_value_t = 0)]
    pub read_extend_duration_ms: u64,
}

#[derive(clap::Args, Debug, Clone)]
pub struct ConnectionArgs {
    /// Zenoh peer to connect to if using Zenoh protocol
    #[arg(short = 'z', long, global = true)]
    pub zenoh_peer: Option<String>,
    /// If using zenoh in peer mode
    #[arg(long = "peer", default_value = "false", global = true)]
    pub peer_mode: bool,
}
