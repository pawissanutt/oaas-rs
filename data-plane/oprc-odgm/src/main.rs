use std::error::Error;

use envconfig::Envconfig;
use oprc_observability::{TracingConfig, setup_tracing};
use oprc_odgm::{OdgmConfig, create_collection};
use tokio::signal;
use tracing::{debug, info};

fn main() {
    let cpus = num_cpus::get();
    let worker_threads = std::cmp::max(1, cpus);

    println!(
        "Starting tokio runtime with {} worker threads",
        worker_threads
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            // Initialize tracing inside async context (OTLP exporters need Tokio runtime)
            let service_name = std::env::var("OPRC_SERVICE_NAME")
                .unwrap_or_else(|_| "oprc-odgm".to_string());
            let log_level = std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "info".to_string());
            let json_format = std::env::var("LOG_FORMAT")
                .map(|v| v.to_lowercase() == "json")
                .unwrap_or(false);

            let config =
                TracingConfig::from_env(&service_name, &log_level, json_format);
            setup_tracing(config).expect("Failed to setup tracing");

            // Initialize OTLP metrics exporter if configured
            if let Err(e) = oprc_observability::init_otlp_metrics_if_configured(
                &service_name,
            ) {
                eprintln!("Failed to initialize OTLP metrics exporter: {}", e);
            }

            info!(
                "Tokio runtime started with {} worker threads",
                worker_threads
            );
            start().await.unwrap()
        });
}

async fn start() -> Result<(), Box<dyn Error>> {
    let conf = OdgmConfig::init_from_env()?;
    debug!("use odgm config: {:?}", conf);
    let odgm = oprc_odgm::start_server(&conf, None).await?.0;

    create_collection(odgm.clone(), &conf).await;

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            eprintln!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
    info!("starting a clean up for shutdown");
    odgm.close().await;
    info!("done clean up");
    Ok(())
}
