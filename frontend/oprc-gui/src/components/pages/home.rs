//! Home page component

use crate::Route;
use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    rsx! {
        div { class: "container mx-auto p-6",
            h1 { class: "text-3xl font-bold mb-4 text-gray-900 dark:text-gray-100", "OaaS-RS Console" }
            p { class: "text-gray-600 dark:text-gray-400 mb-4",
                "Welcome to the OaaS-RS management console."
            }
            div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                div { class: "border border-gray-200 dark:border-gray-700 rounded p-4 hover:shadow-lg transition bg-white dark:bg-gray-800",
                    h2 { class: "text-xl font-semibold mb-2 text-gray-900 dark:text-gray-100", "Quick Actions" }
                    ul { class: "space-y-2",
                        li { Link { to: Route::Invoke {}, class: "text-blue-600 dark:text-blue-400 hover:underline", "Invoke Functions" } }
                        li { Link { to: Route::Objects {}, class: "text-blue-600 dark:text-blue-400 hover:underline", "Browse Objects" } }
                        li { Link { to: Route::Deployments {}, class: "text-blue-600 dark:text-blue-400 hover:underline", "View Deployments" } }
                        li { Link { to: Route::Topology {}, class: "text-blue-600 dark:text-blue-400 hover:underline", "Topology Graph" } }
                    }
                }
                div { class: "border border-gray-200 dark:border-gray-700 rounded p-4 bg-white dark:bg-gray-800",
                    h2 { class: "text-xl font-semibold mb-2 text-gray-900 dark:text-gray-100", "System Status" }
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Status checks coming soon..." }
                }
            }
        }
    }
}
