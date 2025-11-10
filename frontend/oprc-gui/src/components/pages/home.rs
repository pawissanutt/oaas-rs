//! Home page component

use crate::Route;
use crate::SystemHealthSnapshot;
use crate::proxy_system_health;
use dioxus::prelude::*;

#[component]
pub fn Home() -> Element {
    let mut snapshot = use_signal(|| None::<SystemHealthSnapshot>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);
            match proxy_system_health().await {
                Ok(s) => snapshot.set(Some(s)),
                Err(e) => error_msg.set(Some(format!("{e}"))),
            }
            loading.set(false);
        });
    });

    let overall_badge = move |status: &str| -> Element {
        let cls = match status {
            "healthy" => {
                "px-2 py-1 rounded bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300 text-xs font-semibold"
            }
            "degraded" => {
                "px-2 py-1 rounded bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300 text-xs font-semibold"
            }
            _ => {
                "px-2 py-1 rounded bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300 text-xs font-semibold"
            }
        };
        rsx! { span { class: cls, "{status}" } }
    };

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
                    if loading() {
                        div { class: "text-sm text-gray-500 dark:text-gray-400", "Loading health..." }
                    } else if let Some(err) = error_msg() {
                        div { class: "text-sm text-red-600 dark:text-red-400", "Error: {err}" }
                    } else if let Some(snap) = snapshot() {
                        div { class: "flex items-center gap-2 mb-4",
                            span { class: "text-sm text-gray-600 dark:text-gray-400", "Overall:" }
                            { overall_badge(&snap.overall_status) }
                            span { class: "text-xs text-gray-500 dark:text-gray-400", "Updated {snap.timestamp}" }
                        }
                        if !snap.environments.is_empty() {
                            div { class: "space-y-1",
                                for env in snap.environments.iter() {
                                    div { class: "flex items-center justify-between text-sm border border-gray-100 dark:border-gray-700 rounded px-2 py-1",
                                        span { class: "font-medium text-gray-800 dark:text-gray-200", "{env.name}" }
                                        span { class: {
                                                let c = match env.status.as_str() {
                                                    "healthy" => "bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300",
                                                    "degraded" => "bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300",
                                                    _ => "bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300",
                                                };
                                                format!("px-2 py-0.5 rounded text-xs font-semibold {}", c)
                                            }, "{env.status}" }
                                    }
                                }
                            }
                        } else {
                            div { class: "text-xs text-gray-500 dark:text-gray-400", "No environments reported" }
                        }
                    } else {
                        div { class: "text-sm text-gray-500 dark:text-gray-400", "No health data" }
                    }
                }
            }
        }
    }
}
