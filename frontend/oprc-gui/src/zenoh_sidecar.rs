#![cfg(feature = "server")]

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

// Multi-thread Tokio runtime (1 worker) dedicated to zenoh operations.
pub static ZENOH_SIDE_RT: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .thread_name("zenoh-sidecar")
        .build()
        .expect("failed to build zenoh sidecar runtime")
});
