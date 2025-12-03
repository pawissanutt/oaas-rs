//! OaaS-RS Console - CSR frontend
//!
//! Architecture:
//! - Pure CSR rendering (no SSR/LiveView/Server Functions)
//! - API calls go to PM REST endpoints (same origin)
//! - PM serves static assets and proxies to backend services

mod api;
mod components;
mod config;
mod types;

// Re-export server functions so they're accessible from components
pub use api::deployments::proxy_deployments;
pub use api::health::SystemHealthSnapshot;
pub use api::health::proxy_system_health;
pub use api::invoke::proxy_invoke;
pub use api::objects::{
    proxy_get_package, proxy_list_classes, proxy_list_objects,
    proxy_list_objects_all_partitions, proxy_object_delete, proxy_object_get,
    proxy_object_put,
};
pub use api::packages::proxy_packages;
pub use api::topology::proxy_topology;

use components::{Deployments, Home, Navbar, Objects, Packages, Topology};
use dioxus::prelude::*;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ROUTES
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Navbar)]
        #[route("/")]
        Home {},
        #[route("/objects")]
        Objects {},
        #[route("/deployments")]
        Deployments {},
        #[route("/topology")]
        Topology {},
    #[route("/packages")]
    Packages {},
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// ASSETS
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const CYTOSCAPE_JS: Asset = asset!("/assets/cytoscape-graph.js");

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// APP ENTRY
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

fn main() {
    // #[cfg(feature = "server")]
    // let handle = std::thread::spawn(move || {
    //     let runtime = tokio::runtime::Builder::new_current_thread()
    //         .enable_all()
    //         .build()
    //         .unwrap();
    //     runtime.block_on(async {
    //         loop {
    //             // data that will always update
    //             tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    //             *COUNTER_SINCE_APP_STARTED.lock().unwrap() += 1;
    //         }
    //     });
    // });
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Stylesheet { href: TAILWIND_CSS }
        document::Stylesheet { href: MAIN_CSS }

        // Cytoscape.js CDN
        document::Script { src: "https://cdnjs.cloudflare.com/ajax/libs/cytoscape/3.30.1/cytoscape.min.js" }
        // Our custom Cytoscape initialization
        document::Script { src: CYTOSCAPE_JS }

        Router::<Route> {}
    }
}
