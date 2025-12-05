//! Deployments page component with CRUD operations

use crate::api::deployments::{apply_deployment, delete_deployment};
use crate::components::{
    DeploymentYamlEditor, RawDataModal, RelativeTimeFromDt, ValidationResult,
    to_json_string,
};
use crate::proxy_deployments;
use dioxus::prelude::*;

#[component]
pub fn Deployments() -> Element {
    // List state
    let mut deployments =
        use_signal(|| None::<Vec<crate::types::OClassDeployment>>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut search_filter = use_signal(|| "".to_string());

    // Modal state
    let mut show_create_modal = use_signal(|| false);
    let mut yaml_content = use_signal(|| DEFAULT_DEPLOYMENT_YAML.to_string());
    let mut modal_error = use_signal(|| None::<String>);
    let mut modal_loading = use_signal(|| false);
    let mut yaml_valid = use_signal(|| false);

    // Delete confirmation state
    let mut delete_confirm = use_signal(|| None::<String>); // Deployment key to delete

    // Raw data modal state
    let mut show_raw_modal = use_signal(|| false);
    let mut raw_modal_deployment =
        use_signal(|| None::<crate::types::OClassDeployment>);

    // Load deployments
    let refresh_deployments = move || {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);
            match proxy_deployments().await {
                Ok(data) => deployments.set(Some(data)),
                Err(e) => error_msg.set(Some(format!("Error: {}", e))),
            }
            loading.set(false);
        });
    };

    // Auto-fetch on mount
    use_effect(move || {
        refresh_deployments();
    });

    // Apply deployment handler
    let apply_handler = move |_| {
        let content = yaml_content();

        spawn(async move {
            modal_loading.set(true);
            modal_error.set(None);

            match apply_deployment(&content).await {
                Ok(resp) => {
                    show_create_modal.set(false);
                    yaml_content.set(DEFAULT_DEPLOYMENT_YAML.to_string());
                    // Refresh the list
                    match proxy_deployments().await {
                        Ok(data) => deployments.set(Some(data)),
                        Err(e) => error_msg
                            .set(Some(format!("Refresh failed: {}", e))),
                    }
                    if let Some(msg) = resp.message {
                        tracing::info!("Deployment applied: {}", msg);
                    }
                }
                Err(e) => {
                    modal_error.set(Some(e.to_string()));
                }
            }

            modal_loading.set(false);
        });
    };

    // Delete handler
    let confirm_delete_handler = move |_| {
        if let Some(dep_key) = delete_confirm() {
            spawn(async move {
                loading.set(true);
                match delete_deployment(&dep_key).await {
                    Ok(_) => {
                        delete_confirm.set(None);
                        // Refresh the list
                        match proxy_deployments().await {
                            Ok(data) => deployments.set(Some(data)),
                            Err(e) => error_msg
                                .set(Some(format!("Refresh failed: {}", e))),
                        }
                    }
                    Err(e) => {
                        error_msg.set(Some(format!("Delete failed: {}", e)));
                        delete_confirm.set(None);
                    }
                }
                loading.set(false);
            });
        }
    };

    // Filter deployments
    let filtered_deployments = move || {
        let filter = search_filter().to_lowercase();
        deployments().map(|deps| {
            if filter.is_empty() {
                deps
            } else {
                deps.into_iter()
                    .filter(|d| {
                        d.key.to_lowercase().contains(&filter)
                            || d.class_key.to_lowercase().contains(&filter)
                            || d.package_name.to_lowercase().contains(&filter)
                    })
                    .collect()
            }
        })
    };

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
            // Header with title and actions
            div { class: "flex justify-between items-center mb-6",
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-gray-100", "🚀 Deployments" }
                button {
                    class: "px-4 py-2 bg-green-600 text-white rounded-md hover:bg-green-700 transition-colors flex items-center gap-2",
                    onclick: move |_| {
                        yaml_content.set(DEFAULT_DEPLOYMENT_YAML.to_string());
                        modal_error.set(None);
                        show_create_modal.set(true);
                    },
                    "+ New Deployment"
                }
            }

            // Search bar
            div { class: "mb-6",
                input {
                    class: "w-full md:w-96 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                    r#type: "text",
                    placeholder: "🔍 Search deployments...",
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

            if loading() {
                div { class: "text-center py-12",
                    div { class: "inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 dark:border-blue-400" }
                    p { class: "mt-2 text-gray-600 dark:text-gray-400", "Loading deployments..." }
                }
            } else if let Some(deps) = filtered_deployments() {
                if deps.is_empty() {
                    div { class: "text-center py-12 bg-white dark:bg-gray-800 rounded-lg shadow",
                        p { class: "text-gray-500 dark:text-gray-400", "No deployments found" }
                    }
                } else {
                    div { class: "space-y-4",
                        for deployment in deps.iter() {
                            {
                                let dep_key_for_delete = deployment.key.clone();
                                let dep_for_raw = deployment.clone();
                                rsx! {
                                    div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6 hover:shadow-lg transition group",
                                        // Header
                                        div { class: "flex justify-between items-start mb-4",
                                            div {
                                                h2 { class: "text-xl font-semibold text-gray-800 dark:text-gray-100", "🚀 {deployment.key}" }
                                                p { class: "text-sm text-gray-600 dark:text-gray-400 mt-1",
                                                    "Package: "
                                                    span { class: "font-mono", "{deployment.package_name}" }
                                                    " • Class: "
                                                    span { class: "font-mono", "{deployment.class_key}" }
                                                }
                                            }
                                            div { class: "flex items-center gap-3",
                                                {status_badge(&deployment.condition)}
                                                // View raw button
                                                button {
                                                    class: "p-1 text-gray-400 hover:text-blue-600 dark:text-gray-500 dark:hover:text-blue-400 opacity-0 group-hover:opacity-100 transition-all",
                                                    title: "View raw data",
                                                    onclick: move |_| {
                                                        raw_modal_deployment.set(Some(dep_for_raw.clone()));
                                                        show_raw_modal.set(true);
                                                    },
                                                    "📄"
                                                }
                                                button {
                                                    class: "p-1 text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300 opacity-0 group-hover:opacity-100 transition-opacity",
                                                    title: "Delete deployment",
                                                    onclick: move |_| delete_confirm.set(Some(dep_key_for_delete.clone())),
                                                    "🗑️"
                                                }
                                            }
                                        }

                                        // NFR Requirements - only show if any values present
                                        if deployment.nfr_requirements.min_throughput_rps.is_some()
                                            || deployment.nfr_requirements.availability.is_some()
                                            || deployment.nfr_requirements.cpu_utilization_target.is_some()
                                        {
                                            div { class: "bg-gray-50 dark:bg-gray-900 rounded p-4 mb-4",
                                                h4 { class: "text-sm font-medium text-gray-700 dark:text-gray-300 mb-2", "NFR Requirements" }
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
                                        }

                                        // Target environments
                                        if !deployment.target_envs.is_empty() {
                                            div { class: "mb-4",
                                                span { class: "text-sm text-gray-600 dark:text-gray-400", "Target Envs: " }
                                                for env in deployment.target_envs.iter() {
                                                    span { class: "inline-block px-2 py-0.5 text-xs bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 rounded mr-1",
                                                        "{env}"
                                                    }
                                                }
                                            }
                                        }

                                        // Timestamps
                                        if let Some(created) = deployment.created_at {
                                            div { class: "mt-4 pt-4 border-t border-gray-200 dark:border-gray-700 text-xs text-gray-500 dark:text-gray-400 flex flex-wrap gap-1",
                                                RelativeTimeFromDt {
                                                    datetime: created,
                                                    prefix: "Created: "
                                                }
                                                if let Some(updated) = deployment.updated_at {
                                                    span { " • " }
                                                    RelativeTimeFromDt {
                                                        datetime: updated,
                                                        prefix: "Updated: "
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

        // Create Deployment Modal
        if show_create_modal() {
            div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                onclick: move |_| show_create_modal.set(false),
                div {
                    class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-4xl max-h-[90vh] overflow-hidden",
                    onclick: move |e| e.stop_propagation(),

                    // Modal header
                    div { class: "px-6 py-4 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center",
                        h2 { class: "text-xl font-semibold text-gray-900 dark:text-gray-100", "🚀 Create Deployment" }
                        button {
                            class: "text-gray-500 hover:text-gray-700 dark:hover:text-gray-300 text-xl",
                            onclick: move |_| show_create_modal.set(false),
                            "✕"
                        }
                    }

                    // Modal body
                    div { class: "p-6 overflow-y-auto max-h-[calc(90vh-140px)]",
                        // Error display
                        if let Some(err) = modal_error() {
                            div { class: "mb-4 bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded",
                                "{err}"
                            }
                        }

                        // YAML Editor with validation
                        div { class: "mb-4",
                            label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2",
                                "Deployment YAML"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mb-2",
                                "Define the deployment configuration. The editor validates against the OClassDeployment schema."
                            }
                            DeploymentYamlEditor {
                                value: yaml_content,
                                placeholder: "# Enter deployment YAML here...",
                                height: "h-80",
                                on_validation_change: move |result: ValidationResult| {
                                    yaml_valid.set(result.is_valid());
                                }
                            }
                        }
                    }

                    // Modal footer
                    div { class: "px-6 py-4 border-t border-gray-200 dark:border-gray-700 flex justify-between items-center",
                        // Validation hint
                        div { class: "text-sm",
                            if !yaml_valid() {
                                span { class: "text-amber-600 dark:text-amber-400",
                                    "⚠️ Fix validation errors before applying"
                                }
                            }
                        }
                        div { class: "flex gap-3",
                            button {
                                class: "px-4 py-2 bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-md hover:bg-gray-300 dark:hover:bg-gray-500 transition-colors",
                                onclick: move |_| show_create_modal.set(false),
                                "Cancel"
                            }
                            button {
                                class: "px-6 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:bg-gray-400 disabled:cursor-not-allowed transition-colors flex items-center gap-2",
                                disabled: modal_loading() || !yaml_valid(),
                                title: if !yaml_valid() { "Fix validation errors first" } else { "Apply deployment" },
                                onclick: apply_handler,
                                if modal_loading() {
                                    span { class: "inline-block animate-spin h-4 w-4 border-2 border-white border-t-transparent rounded-full" }
                                }
                                "Apply Deployment"
                            }
                        }
                    }
                }
            }
        }

        // Delete Confirmation Modal
        if let Some(dep_key) = delete_confirm() {
            div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                onclick: move |_| delete_confirm.set(None),
                div {
                    class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl p-6 max-w-md",
                    onclick: move |e| e.stop_propagation(),

                    h2 { class: "text-xl font-semibold text-gray-900 dark:text-gray-100 mb-4", "Delete Deployment?" }
                    p { class: "text-gray-600 dark:text-gray-400 mb-6",
                        "Are you sure you want to delete "
                        span { class: "font-mono font-semibold", "{dep_key}" }
                        "? This action cannot be undone."
                    }

                    div { class: "flex justify-end gap-3",
                        button {
                            class: "px-4 py-2 bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-md hover:bg-gray-300 dark:hover:bg-gray-500 transition-colors",
                            onclick: move |_| delete_confirm.set(None),
                            "Cancel"
                        }
                        button {
                            class: "px-4 py-2 bg-red-600 text-white rounded-md hover:bg-red-700 transition-colors",
                            onclick: confirm_delete_handler,
                            "Delete"
                        }
                    }
                }
            }
        }

        // Raw Data Modal
        if let Some(dep) = raw_modal_deployment() {
            RawDataModal {
                show: show_raw_modal,
                json_data: to_json_string(&dep),
                title: "Deployment Definition"
            }
        }
    }
}

const DEFAULT_DEPLOYMENT_YAML: &str = r#"key: my-deployment
class_key: MyClass
package_name: my-package

# Target environments (optional)
target_envs: []

# Non-Functional Requirements
nfr_requirements:
  # min_throughput_rps: 100
  # availability: 0.99
  # cpu_utilization_target: 0.7
"#;
