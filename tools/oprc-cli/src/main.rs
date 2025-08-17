use clap::Parser;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

use oprc_cli::OprcCli;

#[tokio::main]
async fn main() {
    init_log();
    let cli = OprcCli::parse();
    oprc_cli::run(cli).await
}

fn init_log() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::WARN.into())
                .with_env_var("OPRC_LOG")
                .from_env_lossy(),
        )
        .init();
}
