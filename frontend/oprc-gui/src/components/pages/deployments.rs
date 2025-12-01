//! Deployments page component (uses OClassDeployment from models)

use crate::proxy_deployments;
use dioxus::prelude::*;

#[component]
pub fn Deployments() -> Element {
    let mut deployments =
        use_signal(|| None::<Vec<crate::types::OClassDeployment>>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Auto-fetch on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            match proxy_deployments().await {
                Ok(data) => deployments.set(Some(data)),
                Err(e) => error_msg.set(Some(format!("Error: {}", e))),
            }
            loading.set(false);
        });
    });

    let status_badge = |condition: &crate::types::DeploymentCondition| {
        let (color, text) = match condition {
            crate::types::DeploymentCondition::Running => (
                "bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300",
                "Running",
            ),
            crate::types::DeploymentCondition::Deploying => (
                "bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300",
                "Deploying",
            ),
            crate::types::DeploymentCondition::Pending => (
                "bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300",
                "Pending",
            ),
            crate::types::DeploymentCondition::Down => (
                "bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300",
                "Down",
            ),
            crate::types::DeploymentCondition::Deleted => (
                "bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300",
                "Deleted",
            ),
        };
        rsx! {
            span { class: "px-2 py-1 text-xs font-semibold rounded {color}",
                "{text}"
            }
        }
    };

    rsx! {
        div { class: "container mx-auto p-6",
            h1 { class: "text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100", "Deployments" }

            if loading() {
                div { class: "text-center py-12",
                    div { class: "inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 dark:border-blue-400" }
                    p { class: "mt-2 text-gray-600 dark:text-gray-400", "Loading deployments..." }
                }
            } else if let Some(err) = error_msg() {
                div { class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded",
                    "{err}"
                }
            } else if let Some(deps) = deployments() {
                div { class: "space-y-4",
                    for deployment in deps.iter() {
                        div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6 hover:shadow-lg transition",
                            // Header
                            div { class: "flex justify-between items-start mb-4",
                                div {
                                    h2 { class: "text-xl font-semibold text-gray-800 dark:text-gray-100", "{deployment.key}" }
                                    p { class: "text-sm text-gray-600 dark:text-gray-400 mt-1",
                                        "Package: "
                                        span { class: "font-mono", "{deployment.package_name}" }
                                        " • Class: "
                                        span { class: "font-mono", "{deployment.class_key}" }
                                    }
                                }
                                {status_badge(&deployment.condition)}
                            }

                            // NFR Requirements
                            div { class: "bg-gray-50 dark:bg-gray-900 rounded p-4 mb-4",
                                div { class: "grid grid-cols-1 md:grid-cols-3 gap-4 text-sm",
                                    if let Some(throughput) = deployment.nfr_requirements.min_throughput_rps {
                                        div {
                                            span { class: "text-gray-600 dark:text-gray-400", "Min Throughput: " }
                                            span { class: "font-mono text-gray-900 dark:text-gray-100", "{throughput} rps" }
                                        }
                                    }
                                    if let Some(availability) = deployment.nfr_requirements.availability {
                                        div {
                                            span { class: "text-gray-600 dark:text-gray-400", "Availability: " }
                                            span { class: "font-mono text-gray-900 dark:text-gray-100", "{(availability * 100.0):.2}%" }
                                        }
                                    }
                                    if let Some(cpu) = deployment.nfr_requirements.cpu_utilization_target {
                                        div {
                                            span { class: "text-gray-600 dark:text-gray-400", "CPU Target: " }
                                            span { class: "font-mono text-gray-900 dark:text-gray-100", "{(cpu * 100.0):.0}%" }
                                        }
                                    }
                                }
                            }

                            // Timestamps
                            if let Some(ref created) = deployment.created_at {
                                div { class: "mt-4 pt-4 border-t border-gray-200 dark:border-gray-700 text-xs text-gray-500 dark:text-gray-400",
                                    "Created: {created}"
                                    if let Some(ref updated) = deployment.updated_at {
                                        " • Updated: {updated}"
                                    }
                                }
                            }
                        }
                    }

                    if deps.is_empty() {
                        div { class: "text-center py-12 bg-white dark:bg-gray-800 rounded-lg shadow",
                            p { class: "text-gray-500 dark:text-gray-400", "No deployments found" }
                        }
                    }
                }
            }
        }
    }
}
