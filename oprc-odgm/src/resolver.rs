use std::sync::Arc;

use flare_dht::pool::AddrResolver;

use crate::metadata::OprcMetaManager;

struct OprcAddrResolver {
    metadata_manager: Arc<OprcMetaManager>,
}

#[async_trait::async_trait]
impl AddrResolver for OprcAddrResolver {
    async fn resolve(&self, node_id: u64) -> Option<String> {
        return self
            .metadata_manager
            .membership
            .get(&node_id)
            .map(|node| node.node_addr.clone());
    }
}
