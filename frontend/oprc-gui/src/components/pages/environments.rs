//! Environments (Clusters) page component

use crate::api::environments::proxy_environments;
use crate::components::{to_json_string, RawDataModal, RelativeTimeFromDt};
use dioxus::prelude::*;
use oprc_models::ClusterInfo;

#[component]
pub fn Environments() -> Element {
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut environments = use_signal(|| Vec::<ClusterInfo>::new());
    let mut search_filter = use_signal(|| "".to_string());

    // Raw data modal state
    let mut show_raw_modal = use_signal(|| false);
    let mut raw_modal_env = use_signal(|| None::<ClusterInfo>);

    // Load environments
    let refresh_environments = move || {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);

            match proxy_environments().await {
                Ok(data) => environments.set(data),
                Err(e) => error_msg.set(Some(format!("Error: {}", e))),
            }
            loading.set(false);
        });
    };

    // Auto-fetch on mount
    use_effect(move || {
        refresh_environments();
    });

    // Filter environments
    let filtered_environments = move || {
        let filter = search_filter().to_lowercase();
        environments()
            .into_iter()
            .filter(|e| {
                if filter.is_empty() {
                    true
                } else {
                    e.name.to_lowercase().contains(&filter)
                        || e.health.status.to_lowercase().contains(&filter)
                }
            })
            .collect::<Vec<_>>()
    };

    let status_badge = |status: &str| {
        let (color, icon) = match status.to_lowercase().as_str() {
            "healthy" => (
                "bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300",
                "✓",
            ),
            "degraded" => (
                "bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300",
                "⚠",
            ),
            "unhealthy" | "down" => (
                "bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300",
                "✗",
            ),
            _ => (
                "bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300",
                "?",
            ),
        };
        rsx! {
            span { class: "px-2 py-1 text-xs font-semibold rounded {color}",
                "{icon} {status}"
            }
        }
    };

    rsx! {
        div { class: "container mx-auto p-6",
            // Header
            div { class: "flex justify-between items-center mb-6",
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-gray-100", "🌍 Environments" }
                button {
                    class: "px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors flex items-center gap-2",
                    onclick: move |_| refresh_environments(),
                    "↻ Refresh"
                }
            }

            // Search bar
            div { class: "mb-6",
                input {
                    class: "w-full md:w-96 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                    r#type: "text",
                    placeholder: "🔍 Search environments...",
                    value: "{search_filter}",
                    oninput: move |e| search_filter.set(e.value())
                }
            }

            // Error display
            if let Some(err) = error_msg() {
                div { class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded mb-6",
                    "{err}"
                }
            }

            // Loading state
            if loading() {
                div { class: "text-center py-12",
                    div { class: "inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 dark:border-blue-400" }
                    p { class: "mt-2 text-gray-600 dark:text-gray-400", "Loading environments..." }
                }
            } else {
                {
                    let envs = filtered_environments();
                    if envs.is_empty() {
                        rsx! {
                            div { class: "text-center py-12 bg-white dark:bg-gray-800 rounded-lg shadow",
                                p { class: "text-gray-500 dark:text-gray-400", "No environments found" }
                                p { class: "text-sm text-gray-400 dark:text-gray-500 mt-2",
                                    "Environments are connected CRM clusters"
                                }
                            }
                        }
                    } else {
                        rsx! {
                            // Stats
                            div { class: "mb-4 text-sm text-gray-600 dark:text-gray-400",
                                {
                                    let healthy = envs.iter().filter(|e| e.health.status.to_lowercase() == "healthy").count();
                                    let total = envs.len();
                                    format!("{} environment(s) • {} healthy", total, healthy)
                                }
                            }

                            // Environment cards
                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                for env in envs.iter() {
                                    {
                                        let env_for_raw = env.clone();
                                        rsx! {
                                            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-5 hover:shadow-lg transition group",
                                                // Header
                                                div { class: "flex justify-between items-start mb-4",
                                                    h3 { class: "font-semibold text-lg text-gray-900 dark:text-gray-100", "🌍 {env.name}" }
                                                    div { class: "flex items-center gap-2",
                                                        {status_badge(&env.health.status)}
                                                        // View raw button
                                                        button {
                                                            class: "p-1 text-gray-400 hover:text-blue-600 dark:text-gray-500 dark:hover:text-blue-400 opacity-0 group-hover:opacity-100 transition-all",
                                                            title: "View raw data",
                                                            onclick: move |_| {
                                                                raw_modal_env.set(Some(env_for_raw.clone()));
                                                                show_raw_modal.set(true);
                                                            },
                                                            "📄"
                                                        }
                                                    }
                                                }

                                                // Health details
                                                div { class: "bg-gray-50 dark:bg-gray-900 rounded p-4 space-y-2 text-sm",
                                                    // CRM Version
                                                    if let Some(version) = &env.health.crm_version {
                                                        div { class: "flex justify-between",
                                                            span { class: "text-gray-600 dark:text-gray-400", "CRM Version" }
                                                            span { class: "font-mono text-gray-900 dark:text-gray-100", "{version}" }
                                                        }
                                                    }

                                                    // Nodes
                                                    if let (Some(ready), Some(total)) = (env.health.ready_nodes, env.health.node_count) {
                                                        div { class: "flex justify-between",
                                                            span { class: "text-gray-600 dark:text-gray-400", "Nodes" }
                                                            span { class: "font-mono text-gray-900 dark:text-gray-100", "{ready}/{total} ready" }
                                                        }
                                                    }

                                                    // Availability
                                                    if let Some(availability) = env.health.availability {
                                                        div { class: "flex justify-between",
                                                            span { class: "text-gray-600 dark:text-gray-400", "Availability" }
                                                            span {
                                                                class: "font-mono",
                                                                class: if availability >= 0.99 { "text-green-600 dark:text-green-400" }
                                                                       else if availability >= 0.95 { "text-yellow-600 dark:text-yellow-400" }
                                                                       else { "text-red-600 dark:text-red-400" },
                                                                "{(availability * 100.0):.2}%"
                                                            }
                                                        }
                                                    }

                                                    // Last seen
                                                    div { class: "flex justify-between",
                                                        span { class: "text-gray-600 dark:text-gray-400", "Last Seen" }
                                                        RelativeTimeFromDt {
                                                            datetime: env.health.last_seen,
                                                            class: "text-gray-900 dark:text-gray-100"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Raw Data Modal
        if let Some(env) = raw_modal_env() {
            RawDataModal {
                show: show_raw_modal,
                json_data: to_json_string(&env),
                title: "Environment Details"
            }
        }
    }
}
