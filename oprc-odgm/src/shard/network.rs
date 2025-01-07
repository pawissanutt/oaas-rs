use std::sync::Arc;

use flare_dht::shard::KvShard;

struct ShardNetwork<T: KvShard> {
    z_session: zenoh::Session,
    shard: Arc<T>,
}

impl<T: KvShard> ShardNetwork<T> {
    pub fn new(z_session: zenoh::Session, shard: Arc<T>) -> Self {
        Self { z_session, shard }
    }
}
