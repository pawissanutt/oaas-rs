use std::sync::Arc;

use crate::cluster::ObjectDataGridManager;
use oprc_zenoh::util::{Handler, ManagedConfig, declare_managed_queryable};
use serde_json::json;
use zenoh::query::Query;

use super::model::CapErr;
use super::provider::CapabilitiesProvider;

#[derive(Clone)]
struct CapsHandler {
    provider: CapabilitiesProvider,
}

#[async_trait::async_trait]
impl Handler<Query> for CapsHandler {
    async fn handle(&self, query: Query) {
        // Expect key like: oprc/<cls>/<partition>/shards/<shard_id>/capabilities
        let key = query.key_expr().to_string();
        let parts: Vec<&str> = key.split('/').collect();
        let result = async {
            if parts.len() < 6 {
                return Err(CapErr::BadRequest);
            }
            // parts[0] = oprc, [1]=cls, [2]=partition, [3]=shards, [4]=shard_id, [5]=capabilities (or more if wildcard)
            let cls = parts.get(1).ok_or(CapErr::BadRequest)?.to_string();
            let partition: u32 = parts
                .get(2)
                .ok_or(CapErr::BadRequest)?
                .parse()
                .map_err(|_| CapErr::BadRequest)?;
            let shard_id: u64 = parts
                .get(4)
                .ok_or(CapErr::BadRequest)?
                .parse()
                .map_err(|_| CapErr::BadRequest)?;

            self.provider
                .get_shard_capabilities(&cls, partition, shard_id)
                .await
        }
        .await;

        match result {
            Ok(caps) => {
                let _ =
                    query.reply(&key, serde_json::to_vec(&caps).unwrap()).await;
            }
            Err(CapErr::NotFound) => {
                let _ = query
                    .reply(
                        &key,
                        serde_json::to_vec(&json!({"error":"not_found"}))
                            .unwrap(),
                    )
                    .await;
            }
            Err(CapErr::BadRequest) => {
                let _ = query
                    .reply(
                        &key,
                        serde_json::to_vec(&json!({"error":"bad_request"}))
                            .unwrap(),
                    )
                    .await;
            }
            Err(CapErr::Internal(e)) => {
                let _ = query
                    .reply(
                        &key,
                        serde_json::to_vec(
                            &json!({"error":"internal","message":e}),
                        )
                        .unwrap(),
                    )
                    .await;
            }
        }
    }
}

/// Start capabilities service: register a per-shard queryable
pub async fn start_caps_service(
    z_session: zenoh::Session,
    odgm: Arc<ObjectDataGridManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Read env for flags
    let event_pipeline_v2 = std::env::var("ODGM_EVENT_PIPELINE_V2")
        .ok()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let bridge_mode = std::env::var("ODGM_EVENT_PIPELINE_BRIDGE")
        .ok()
        .map(|v| v == "true" || v == "1")
        .unwrap_or(true);
    // Build version string from cargo pkg version if available
    let version = option_env!("CARGO_PKG_VERSION")
        .unwrap_or("dev")
        .to_string();

    let provider = CapabilitiesProvider::new(
        odgm,
        event_pipeline_v2,
        bridge_mode,
        version,
    );

    let key = "oprc/**/shards/**/capabilities";
    let handler = CapsHandler { provider };

    let queryable = declare_managed_queryable(
        &z_session,
        ManagedConfig::unbounded(key, 2),
        handler,
    )
    .await
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    // Keep session and queryable alive in a background task
    tokio::spawn(async move {
        let _hold_session = z_session;
        let _hold_queryable = queryable;
        futures::future::pending::<()>().await;
    });

    tracing::info!(%key, "Capabilities queryable registered");
    Ok(())
}
