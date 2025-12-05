//! Layout components

use crate::Route;
use dioxus::prelude::*;

/// Main navigation bar with routing
#[component]
pub fn Navbar() -> Element {
    rsx! {
        div { class: "min-h-screen bg-gray-100 dark:bg-gray-900",
            nav { class: "bg-gray-800 text-white p-4 shadow-md",
                div { class: "container mx-auto flex justify-between items-center",
                    div { class: "flex gap-4",
                        Link { to: Route::Home {}, class: "hover:text-gray-300 transition-colors", "Home" }
                        Link { to: Route::Objects {}, class: "hover:text-gray-300 transition-colors", "Objects" }
                        Link { to: Route::Deployments {}, class: "hover:text-gray-300 transition-colors", "Deployments" }
                        Link { to: Route::Functions {}, class: "hover:text-gray-300 transition-colors", "Functions" }
                        Link { to: Route::Environments {}, class: "hover:text-gray-300 transition-colors", "Envs" }
                        Link { to: Route::Topology {}, class: "hover:text-gray-300 transition-colors", "Topology" }
                        Link { to: Route::Packages {}, class: "hover:text-gray-300 transition-colors", "Packages" }
                    }
                    div {
                        Link { to: Route::Settings {}, class: "hover:text-gray-300 transition-colors", "⚙️" }
                    }
                }
            }
            div { class: "text-gray-900 dark:text-gray-100",
                Outlet::<Route> {}
            }
        }
    }
}
