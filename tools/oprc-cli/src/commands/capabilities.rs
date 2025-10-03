use crate::types::ConnectionArgs;
use oprc_grpc::{data_service_client::DataServiceClient, CapabilitiesRequest};

pub async fn handle_capabilities_command(conn: &ConnectionArgs, json: bool) -> anyhow::Result<()> {
    let url = conn.grpc_url.clone().ok_or_else(|| anyhow::anyhow!("gRPC URL required (set context gateway_url or pass --grpc-url)"))?;
    let mut client = DataServiceClient::connect(url).await?;
    let resp = client.capabilities(CapabilitiesRequest {}).await?.into_inner();
    if json {
        let val = serde_json::json!({
            "string_ids": resp.string_ids,
            "string_entry_keys": resp.string_entry_keys,
            "granular_entry_storage": resp.granular_entry_storage,
        });
        println!("{}", serde_json::to_string_pretty(&val)?);
    } else {
        println!("Capabilities:\n  string_ids: {}\n  string_entry_keys: {}\n  granular_entry_storage: {}",
            resp.string_ids, resp.string_entry_keys, resp.granular_entry_storage);
    }
    Ok(())
}
