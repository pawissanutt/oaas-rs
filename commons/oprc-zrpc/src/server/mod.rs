mod non_sync;
mod sync;

pub use non_sync::ZrpcNonSyncService;
pub use non_sync::ZrpcNonSyncServiceHander;
pub use sync::ZrpcService;
pub use sync::ZrpcServiceHander;

#[derive(Clone)]
pub struct ServerConfig {
    pub service_id: String,
    pub concurrency: u32,
    pub bound_channel: u32,
    pub accept_subfix: bool,
    pub complete: bool,
    pub reply_congestion: zenoh::qos::CongestionControl,
    pub reply_priority: zenoh::qos::Priority,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            service_id: "".to_string(),
            concurrency: 16,
            bound_channel: 0,
            accept_subfix: false,
            complete: true,
            reply_congestion: zenoh::qos::CongestionControl::Block,
            reply_priority: zenoh::qos::Priority::default(),
        }
    }
}
