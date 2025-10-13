use oprc_gateway::{Config, start_server};

fn main() {
    let cpus = num_cpus::get();
    let worker_threads = std::cmp::max(1, cpus);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async { start().await });
}

async fn start() {
    match Config::load_from_env() {
        Ok(conf) => {
            if let Err(e) = start_server(conf).await {
                tracing::error!("Error starting server: {:?}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            tracing::error!("Failed to load config from env: {:?}", e);
            std::process::exit(1);
        }
    }
}
