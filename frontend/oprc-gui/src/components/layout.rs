//! Layout components

use crate::Route;
use dioxus::prelude::*;

/// Main navigation bar with routing - responsive sidebar on mobile
#[component]
pub fn Navbar() -> Element {
    let mut sidebar_open = use_signal(|| false);

    // Navigation items
    let nav_items = [
        (Route::Home {}, "🏠", "Home"),
        (Route::Objects {}, "📦", "Objects"),
        (Route::Deployments {}, "🚀", "Deployments"),
        (Route::Functions {}, "⚡", "Functions"),
        (Route::Environments {}, "🌍", "Envs"),
        (Route::Topology {}, "🔗", "Topology"),
        (Route::Packages {}, "📋", "Packages"),
    ];

    rsx! {
        div { class: "min-h-screen bg-gray-100 dark:bg-gray-900",
            // Mobile header with hamburger menu
            div { class: "md:hidden bg-gray-800 text-white p-3 flex justify-between items-center sticky top-0 z-40",
                button {
                    class: "p-2 hover:bg-gray-700 rounded-lg transition-colors",
                    onclick: move |_| sidebar_open.set(!sidebar_open()),
                    "☰"
                }
                span { class: "font-semibold", "OaaS Console" }
                Link { to: Route::Settings {}, class: "p-2 hover:bg-gray-700 rounded-lg transition-colors", "⚙️" }
            }

            // Mobile sidebar overlay
            if sidebar_open() {
                div {
                    class: "md:hidden fixed inset-0 bg-black/50 z-40",
                    onclick: move |_| sidebar_open.set(false)
                }
            }

            // Mobile sidebar
            div {
                class: format!(
                    "md:hidden fixed top-0 left-0 h-full w-64 bg-gray-800 text-white z-50 transform transition-transform duration-300 ease-in-out {}",
                    if sidebar_open() { "translate-x-0" } else { "-translate-x-full" }
                ),
                div { class: "p-4 border-b border-gray-700 flex justify-between items-center",
                    span { class: "font-bold text-lg", "OaaS Console" }
                    button {
                        class: "p-1 hover:bg-gray-700 rounded transition-colors",
                        onclick: move |_| sidebar_open.set(false),
                        "✕"
                    }
                }
                nav { class: "p-2",
                    for (route, icon, label) in nav_items.iter() {
                        Link {
                            to: route.clone(),
                            class: "flex items-center gap-3 px-4 py-3 hover:bg-gray-700 rounded-lg transition-colors",
                            onclick: move |_| sidebar_open.set(false),
                            span { "{icon}" }
                            span { "{label}" }
                        }
                    }
                    // Settings at bottom
                    div { class: "border-t border-gray-700 mt-4 pt-4",
                        Link {
                            to: Route::Settings {},
                            class: "flex items-center gap-3 px-4 py-3 hover:bg-gray-700 rounded-lg transition-colors",
                            onclick: move |_| sidebar_open.set(false),
                            span { "⚙️" }
                            span { "Settings" }
                        }
                    }
                }
            }

            // Desktop navbar (hidden on mobile)
            nav { class: "hidden md:block bg-gray-800 text-white p-4 shadow-md",
                div { class: "container mx-auto flex justify-between items-center",
                    div { class: "flex gap-4 flex-wrap",
                        for (route, icon, label) in nav_items.iter() {
                            Link {
                                to: route.clone(),
                                class: "hover:text-gray-300 transition-colors flex items-center gap-1",
                                span { class: "hidden lg:inline", "{icon}" }
                                span { "{label}" }
                            }
                        }
                    }
                    div {
                        Link { to: Route::Settings {}, class: "hover:text-gray-300 transition-colors", "⚙️" }
                    }
                }
            }

            // Main content
            div { class: "text-gray-900 dark:text-gray-100",
                Outlet::<Route> {}
            }
        }
    }
}
