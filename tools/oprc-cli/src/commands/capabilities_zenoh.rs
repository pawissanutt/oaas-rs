use crate::types::ConnectionArgs;
use serde_json::Value;
use std::convert::TryInto;
use zenoh::key_expr::KeyExpr;

pub async fn handle_capabilities_zenoh_command(
    conn: &ConnectionArgs,
    cls: &str,
    partition_id: &str,
    shard_id: &str,
    json: bool,
) -> anyhow::Result<()> {
    let session = conn.open_zenoh().await;
    let key = format!(
        "oprc/{}/{}/shards/{}/capabilities",
        cls, partition_id, shard_id
    );
    let ke: KeyExpr = key.try_into().map_err(|_| {
        anyhow::anyhow!("invalid key expression for capabilities")
    })?;
    let replies = session.get(&ke).await.map_err(|_| {
        anyhow::anyhow!("failed to issue zenoh get for capabilities")
    })?;

    // Collect all replies (supports wildcard aggregation)
    let mut vals: Vec<Value> = Vec::new();
    loop {
        match replies.recv_async().await {
            Ok(reply) => match reply.result() {
                Ok(sample) => {
                    let bytes = sample.payload().to_bytes();
                    if let Ok(v) =
                        serde_json::from_slice::<Value>(bytes.as_ref())
                    {
                        vals.push(v);
                    }
                }
                Err(_) => {
                    // Continue collecting other replies
                }
            },
            Err(_) => break,
        }
    }

    if vals.is_empty() {
        return Err(anyhow::anyhow!("no capabilities replies received"));
    }

    if json {
        if vals.len() == 1 {
            println!("{}", serde_json::to_string_pretty(&vals[0])?);
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::Value::Array(vals))?
            );
        }
    } else {
        for v in vals.iter() {
            println!(
                "Capabilities (zenoh):\n  class: {}\n  partition: {}\n  shard_id: {}\n  event_pipeline_v2: {}\n  storage_backend: {}\n  odgm_version: {}\n  features.granular_storage: {}\n  features.bridge_mode: {}",
                v.get("class").and_then(|x| x.as_str()).unwrap_or(""),
                v.get("partition").and_then(|x| x.as_u64()).unwrap_or(0),
                v.get("shard_id").and_then(|x| x.as_u64()).unwrap_or(0),
                v.get("event_pipeline_v2")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                v.get("storage_backend")
                    .and_then(|x| x.as_str())
                    .unwrap_or(""),
                v.get("odgm_version").and_then(|x| x.as_str()).unwrap_or(""),
                v.get("features")
                    .and_then(|f| f.get("granular_storage"))
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
                v.get("features")
                    .and_then(|f| f.get("bridge_mode"))
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
            );
        }
    }

    Ok(())
}
