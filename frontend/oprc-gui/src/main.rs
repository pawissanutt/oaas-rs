//! OaaS-RS Console - CSR frontend with optional server-function proxies
//!
//! Architecture:
//! - CSR rendering (no SSR/LiveView)
//! - Server functions enabled for same-origin API proxying
//! - Dev-mode mocks via OPRC_GUI_DEV_MOCK env
//! - Relays to gateway via OPRC_GATEWAY_BASE_URL

// API module with server functions - always compiled
// On server: full implementation runs
// On client: Dioxus #[post] macro generates HTTP client stubs
mod api;

mod components;
mod config;
mod types;

// Re-export server functions so they're accessible from components
pub use api::deployments::proxy_deployments;
pub use api::health::SystemHealthSnapshot;
pub use api::health::proxy_system_health;
pub use api::invoke::proxy_invoke;
pub use api::objects::proxy_object_get;
pub use api::packages::proxy_packages;
pub use api::topology::proxy_topology;

use components::{
    Deployments, Home, Invoke, Navbar, Objects, Packages, Topology,
};
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
        #[route("/invoke")]
        Invoke {},
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
