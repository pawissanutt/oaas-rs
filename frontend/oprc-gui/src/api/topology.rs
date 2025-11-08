use crate::types::TopologySnapshot;
use dioxus::prelude::*;

#[post("/api/proxy/topology")]
pub async fn proxy_topology() -> Result<TopologySnapshot, ServerFnError> {
    #[cfg(not(feature = "server"))]
    {
        unreachable!()
    }
    #[cfg(feature = "server")]
    {
        if crate::config::is_dev_mock() {
            Ok(crate::api::mock::mock_topology_snapshot())
        } else {
            // Future: query Zenoh admin space
            Err(ServerFnError::new("Topology relay not yet implemented"))
        }
    }
}
