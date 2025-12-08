//! Home page component - Dashboard with stats, health, and quick actions

use crate::Route;
use crate::SystemHealthSnapshot;
use crate::api::deployments::proxy_deployments;
use crate::api::packages::proxy_packages;
use crate::components::RelativeTime;
use crate::proxy_system_health;
use dioxus::prelude::*;
use oprc_models::OClassDeployment;
use oprc_models::OPackage;

/// Dashboard statistics computed from packages and deployments
#[derive(Default, Clone, PartialEq)]
struct DashboardStats {
    total_packages: usize,
    total_classes: usize,
    total_functions: usize,
    total_deployments: usize,
    running_deployments: usize,
    pending_deployments: usize,
    error_deployments: usize,
}

impl DashboardStats {
    fn from_data(
        packages: &[OPackage],
        deployments: &[OClassDeployment],
    ) -> Self {
        let total_packages = packages.len();
        let total_classes: usize =
            packages.iter().map(|p| p.classes.len()).sum();
        // Count function bindings across all classes
        let total_functions: usize = packages
            .iter()
            .flat_map(|p| p.classes.iter())
            .map(|c| c.function_bindings.len())
            .sum();

        let total_deployments = deployments.len();
        let mut running = 0;
        let mut pending = 0;
        let mut error = 0;

        for dep in deployments {
            if let Some(status) = &dep.status {
                // Check if there's an error
                if status.last_error.is_some() {
                    error += 1;
                } else if !status.selected_envs.is_empty() {
                    // Has selected envs = running
                    running += 1;
                } else {
                    pending += 1;
                }
            } else {
                pending += 1;
            }
        }

        Self {
            total_packages,
            total_classes,
            total_functions,
            total_deployments,
            running_deployments: running,
            pending_deployments: pending,
            error_deployments: error,
        }
    }
}

/// Stat card component for displaying a single metric
#[component]
fn StatCard(
    title: &'static str,
    value: usize,
    icon: &'static str,
    color: &'static str,
) -> Element {
    let (bg, text, icon_bg) = match color {
        "blue" => (
            "bg-blue-50 dark:bg-blue-900/20",
            "text-blue-600 dark:text-blue-400",
            "bg-blue-100 dark:bg-blue-800/50",
        ),
        "green" => (
            "bg-green-50 dark:bg-green-900/20",
            "text-green-600 dark:text-green-400",
            "bg-green-100 dark:bg-green-800/50",
        ),
        "purple" => (
            "bg-purple-50 dark:bg-purple-900/20",
            "text-purple-600 dark:text-purple-400",
            "bg-purple-100 dark:bg-purple-800/50",
        ),
        "orange" => (
            "bg-orange-50 dark:bg-orange-900/20",
            "text-orange-600 dark:text-orange-400",
            "bg-orange-100 dark:bg-orange-800/50",
        ),
        _ => (
            "bg-gray-50 dark:bg-gray-900/20",
            "text-gray-600 dark:text-gray-400",
            "bg-gray-100 dark:bg-gray-800/50",
        ),
    };

    rsx! {
        div {
            class: "rounded-lg p-4 border border-gray-200 dark:border-gray-700 {bg}",
            div { class: "flex items-center justify-between",
                div {
                    p { class: "text-sm font-medium text-gray-600 dark:text-gray-400", "{title}" }
                    p { class: "text-2xl font-bold {text}", "{value}" }
                }
                div { class: "w-12 h-12 rounded-full {icon_bg} flex items-center justify-center",
                    span { class: "text-2xl", "{icon}" }
                }
            }
        }
    }
}

/// Quick action button component
#[component]
fn QuickActionButton(
    to: Route,
    icon: &'static str,
    label: &'static str,
    description: &'static str,
) -> Element {
    rsx! {
        Link {
            to: to,
            class: "flex items-center gap-3 p-3 rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors",
            span { class: "text-2xl", "{icon}" }
            div {
                p { class: "font-medium text-gray-900 dark:text-gray-100", "{label}" }
                p { class: "text-xs text-gray-500 dark:text-gray-400", "{description}" }
            }
        }
    }
}

#[component]
pub fn Home() -> Element {
    let mut health_snapshot = use_signal(|| None::<SystemHealthSnapshot>);
    let mut packages = use_signal(Vec::<OPackage>::new);
    let mut deployments = use_signal(Vec::<OClassDeployment>::new);
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);

    // Load all data in parallel on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);

            // Fetch all data in parallel
            let (health_res, pkg_res, dep_res) = futures::join!(
                proxy_system_health(),
                proxy_packages(),
                proxy_deployments()
            );

            match health_res {
                Ok(s) => health_snapshot.set(Some(s)),
                Err(e) => tracing::warn!("Failed to load health: {e}"),
            }

            match pkg_res {
                Ok(snapshot) => packages.set(snapshot.packages),
                Err(e) => tracing::warn!("Failed to load packages: {e}"),
            }

            match dep_res {
                Ok(d) => deployments.set(d),
                Err(e) => tracing::warn!("Failed to load deployments: {e}"),
            }

            loading.set(false);
        });
    });

    let stats = use_memo(move || {
        DashboardStats::from_data(&packages(), &deployments())
    });

    let overall_badge = move |status: &str| -> Element {
        let cls = match status.to_lowercase().as_str() {
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

    // Extract stats for use in rsx
    let st = stats.read();

    rsx! {
        div { class: "container mx-auto p-6 space-y-6",
            // Header
            div { class: "flex items-center justify-between",
                div {
                    h1 { class: "text-3xl font-bold text-gray-900 dark:text-gray-100", "Dashboard" }
                    p { class: "text-gray-600 dark:text-gray-400", "OaaS-RS Management Console" }
                }
                if loading() {
                    div { class: "flex items-center gap-2 text-sm text-gray-500 dark:text-gray-400",
                        span { class: "animate-spin", "⟳" }
                        "Loading..."
                    }
                }
            }

            // Error banner
            if let Some(err) = error_msg() {
                div { class: "p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg",
                    p { class: "text-sm text-red-600 dark:text-red-400", "Error: {err}" }
                }
            }

            // Stats cards row
            div { class: "grid grid-cols-2 lg:grid-cols-4 gap-4",
                StatCard { title: "Packages", value: st.total_packages, icon: "📦", color: "blue" }
                StatCard { title: "Classes", value: st.total_classes, icon: "🏷️", color: "purple" }
                StatCard { title: "Functions", value: st.total_functions, icon: "⚡", color: "orange" }
                StatCard { title: "Deployments", value: st.total_deployments, icon: "🚀", color: "green" }
            }

            // Main content grid
            div { class: "grid grid-cols-1 lg:grid-cols-2 gap-6",
                // Deployment Status Panel
                div { class: "rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-5",
                    h2 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4", "Deployment Status" }
                    if st.total_deployments == 0 {
                        div { class: "text-center py-8 text-gray-500 dark:text-gray-400",
                            p { class: "text-4xl mb-2", "🚀" }
                            p { "No deployments yet" }
                            Link {
                                to: Route::Deployments {},
                                class: "text-blue-600 dark:text-blue-400 hover:underline text-sm",
                                "Create your first deployment →"
                            }
                        }
                    } else {
                        div { class: "space-y-3",
                            // Status breakdown
                            div { class: "flex items-center justify-between p-3 bg-green-50 dark:bg-green-900/20 rounded-lg",
                                div { class: "flex items-center gap-2",
                                    span { class: "w-3 h-3 rounded-full bg-green-500" }
                                    span { class: "text-sm font-medium text-gray-700 dark:text-gray-300", "Running" }
                                }
                                span { class: "text-lg font-bold text-green-600 dark:text-green-400", "{st.running_deployments}" }
                            }
                            div { class: "flex items-center justify-between p-3 bg-yellow-50 dark:bg-yellow-900/20 rounded-lg",
                                div { class: "flex items-center gap-2",
                                    span { class: "w-3 h-3 rounded-full bg-yellow-500" }
                                    span { class: "text-sm font-medium text-gray-700 dark:text-gray-300", "Pending" }
                                }
                                span { class: "text-lg font-bold text-yellow-600 dark:text-yellow-400", "{st.pending_deployments}" }
                            }
                            div { class: "flex items-center justify-between p-3 bg-red-50 dark:bg-red-900/20 rounded-lg",
                                div { class: "flex items-center gap-2",
                                    span { class: "w-3 h-3 rounded-full bg-red-500" }
                                    span { class: "text-sm font-medium text-gray-700 dark:text-gray-300", "Error" }
                                }
                                span { class: "text-lg font-bold text-red-600 dark:text-red-400", "{st.error_deployments}" }
                            }
                            // Progress bar
                            if st.total_deployments > 0 {
                                div { class: "mt-4",
                                    div { class: "h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden flex",
                                        div {
                                            class: "bg-green-500 h-full",
                                            style: "width: {st.running_deployments as f64 / st.total_deployments as f64 * 100.0}%"
                                        }
                                        div {
                                            class: "bg-yellow-500 h-full",
                                            style: "width: {st.pending_deployments as f64 / st.total_deployments as f64 * 100.0}%"
                                        }
                                        div {
                                            class: "bg-red-500 h-full",
                                            style: "width: {st.error_deployments as f64 / st.total_deployments as f64 * 100.0}%"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // System Health Panel
                div { class: "rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-5",
                    h2 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4", "System Health" }
                    if let Some(snap) = health_snapshot() {
                        div { class: "space-y-4",
                            // Overall status
                            div { class: "flex items-center justify-between",
                                div { class: "flex items-center gap-2",
                                    span { class: "text-sm text-gray-600 dark:text-gray-400", "Overall Status:" }
                                    { overall_badge(&snap.overall_status) }
                                }
                                RelativeTime {
                                    timestamp: snap.timestamp.clone(),
                                    prefix: "Updated ",
                                    class: "text-xs text-gray-500 dark:text-gray-400"
                                }
                            }
                            // Environments
                            if !snap.environments.is_empty() {
                                div { class: "space-y-2",
                                    p { class: "text-sm font-medium text-gray-600 dark:text-gray-400", "Environments" }
                                    for env in snap.environments.iter() {
                                        div { class: "flex items-center justify-between p-2 bg-gray-50 dark:bg-gray-700/50 rounded",
                                            span { class: "font-medium text-gray-800 dark:text-gray-200 text-sm", "{env.name}" }
                                            div { class: "flex items-center gap-2",
                                                span {
                                                    class: {
                                                        let c = match env.status.to_lowercase().as_str() {
                                                            "healthy" => "bg-green-500",
                                                            "degraded" => "bg-yellow-500",
                                                            _ => "bg-red-500",
                                                        };
                                                        format!("w-2 h-2 rounded-full {c}")
                                                    }
                                                }
                                                span { class: "text-xs text-gray-600 dark:text-gray-400", "{env.status}" }
                                            }
                                        }
                                    }
                                }
                            } else {
                                div { class: "text-sm text-gray-500 dark:text-gray-400 text-center py-4",
                                    "No environments reported"
                                }
                            }
                        }
                    } else if loading() {
                        div { class: "flex items-center justify-center py-8 text-gray-500 dark:text-gray-400",
                            span { class: "animate-spin mr-2", "⟳" }
                            "Loading health status..."
                        }
                    } else {
                        div { class: "text-center py-8 text-gray-500 dark:text-gray-400",
                            p { class: "text-4xl mb-2", "❓" }
                            p { "Health data unavailable" }
                        }
                    }
                }
            }

            // Quick Actions
            div { class: "rounded-lg border border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800 p-5",
                h2 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100 mb-4", "Quick Actions" }
                div { class: "grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3",
                    QuickActionButton {
                        to: Route::Packages {},
                        icon: "📦",
                        label: "Manage Packages",
                        description: "Create & edit packages"
                    }
                    QuickActionButton {
                        to: Route::Deployments {},
                        icon: "🚀",
                        label: "Deployments",
                        description: "Deploy & monitor"
                    }
                    QuickActionButton {
                        to: Route::Objects {},
                        icon: "🗃️",
                        label: "Browse Objects",
                        description: "View object data"
                    }
                    QuickActionButton {
                        to: Route::Topology {},
                        icon: "🔗",
                        label: "Topology",
                        description: "Visualize graph"
                    }
                }
            }
        }
    }
}
