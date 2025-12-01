use crate::{errors::ApiError, server::AppState};
use axum::{Json, extract::State};
use oprc_grpc::proto::topology::TopologySnapshot;
use tracing::info;

pub async fn get_topology(
    State(state): State<AppState>,
) -> Result<Json<TopologySnapshot>, ApiError> {
    info!("API: Fetching topology");
    let topology = state.crm_manager.get_topology().await.map_err(|e| {
        ApiError::InternalServerError(format!(
            "Failed to fetch topology: {}",
            e
        ))
    })?;
    Ok(Json(topology))
}
