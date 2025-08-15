pub mod config;
pub mod controller;
pub mod crd;
pub mod grpc;
pub mod nfr;
pub mod templates;
pub mod web;
pub mod runtime;
pub mod collections;

use tracing_subscriber::{
    EnvFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

pub fn init_tracing(default_env: &str) {
    let filter = EnvFilter::builder()
    .with_env_var("RUST_LOG")
        .from_env_lossy()
        .add_directive(
            default_env
                .parse()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        );

    let _ = tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(filter)
        .try_init();
}
