//! Layout components

use crate::Route;
use dioxus::prelude::*;

/// Main navigation bar with routing
#[component]
pub fn Navbar() -> Element {
    rsx! {
        nav { class: "bg-gray-800 dark:bg-gray-900 text-white p-4 shadow-md",
            div { class: "container mx-auto flex gap-4",
                Link { to: Route::Home {}, class: "hover:text-gray-300 dark:hover:text-gray-400 transition-colors", "Home" }
                Link { to: Route::Invoke {}, class: "hover:text-gray-300 dark:hover:text-gray-400 transition-colors", "Invoke" }
                Link { to: Route::Objects {}, class: "hover:text-gray-300 dark:hover:text-gray-400 transition-colors", "Objects" }
                Link { to: Route::Deployments {}, class: "hover:text-gray-300 dark:hover:text-gray-400 transition-colors", "Deployments" }
                Link { to: Route::Topology {}, class: "hover:text-gray-300 dark:hover:text-gray-400 transition-colors", "Topology" }
            }
        }
        Outlet::<Route> {}
    }
}
