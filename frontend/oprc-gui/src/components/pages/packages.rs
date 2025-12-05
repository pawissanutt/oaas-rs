//! Packages page component with CRUD operations

use crate::api::packages::{apply_package, delete_package};
use crate::components::{to_json_string, RawDataModal, ValidationResult, YamlEditor};
use crate::proxy_packages;
use crate::types::*;
use dioxus::prelude::*;
use oprc_models::OPackage;

#[component]
pub fn Packages() -> Element {
    // List state
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut snapshot = use_signal(|| None::<PackagesSnapshot>);
    let mut search_filter = use_signal(|| "".to_string());

    // Modal state
    let mut show_create_modal = use_signal(|| false);
    let mut yaml_content = use_signal(|| DEFAULT_PACKAGE_YAML.to_string());
    let mut modal_error = use_signal(|| None::<String>);
    let mut modal_loading = use_signal(|| false);
    let mut also_deploy = use_signal(|| false);
    let mut yaml_valid = use_signal(|| false);

    // Delete confirmation state
    let mut delete_confirm = use_signal(|| None::<String>); // Package name to delete

    // Raw data modal state
    let mut show_raw_modal = use_signal(|| false);
    let mut raw_modal_package = use_signal(|| None::<OPackage>);

    // Load packages
    let refresh_packages = move || {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);
            match proxy_packages().await {
                Ok(data) => {
                    snapshot.set(Some(data));
                }
                Err(e) => {
                    error_msg.set(Some(e.to_string()));
                }
            }
            loading.set(false);
        });
    };

    // Initial load
    use_effect(move || {
        refresh_packages();
    });

    // Apply package handler
    let apply_handler = move |_| {
        let content = yaml_content();
        let deploy = also_deploy();

        spawn(async move {
            modal_loading.set(true);
            modal_error.set(None);

            // If deploy checkbox is checked, we need to handle that
            // For now, just apply the package
            let content_to_apply = if deploy {
                // The PM should handle deployment if deployments are included in the YAML
                content.clone()
            } else {
                content.clone()
            };

            match apply_package(&content_to_apply).await {
                Ok(resp) => {
                    show_create_modal.set(false);
                    yaml_content.set(DEFAULT_PACKAGE_YAML.to_string());
                    also_deploy.set(false);
                    // Refresh the list
                    match proxy_packages().await {
                        Ok(data) => snapshot.set(Some(data)),
                        Err(e) => error_msg
                            .set(Some(format!("Refresh failed: {}", e))),
                    }
                    if let Some(msg) = resp.message {
                        tracing::info!("Package applied: {}", msg);
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
        if let Some(pkg_name) = delete_confirm() {
            spawn(async move {
                loading.set(true);
                match delete_package(&pkg_name).await {
                    Ok(_) => {
                        delete_confirm.set(None);
                        // Refresh the list
                        match proxy_packages().await {
                            Ok(data) => snapshot.set(Some(data)),
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

    // Filter packages
    let filtered_packages = move || {
        let filter = search_filter().to_lowercase();
        snapshot().map(|s| {
            if filter.is_empty() {
                s.packages
            } else {
                s.packages
                    .into_iter()
                    .filter(|p| p.name.to_lowercase().contains(&filter))
                    .collect()
            }
        })
    };

    rsx! {
        div { class: "container mx-auto p-6",
            // Header with title and actions
            div { class: "flex justify-between items-center mb-6",
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-gray-100", "📦 Packages" }
                button {
                    class: "px-4 py-2 bg-green-600 text-white rounded-md hover:bg-green-700 transition-colors flex items-center gap-2",
                    onclick: move |_| {
                        yaml_content.set(DEFAULT_PACKAGE_YAML.to_string());
                        modal_error.set(None);
                        show_create_modal.set(true);
                    },
                    "+ New Package"
                }
            }

            // Search bar
            div { class: "mb-6",
                input {
                    class: "w-full md:w-96 px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                    r#type: "text",
                    placeholder: "🔍 Search packages...",
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
                    p { class: "mt-2 text-gray-600 dark:text-gray-400", "Loading packages..." }
                }
            } else if let Some(packages) = filtered_packages() {
                if packages.is_empty() {
                    div { class: "text-center py-12 bg-white dark:bg-gray-800 rounded-lg shadow",
                        p { class: "text-gray-500 dark:text-gray-400", "No packages found" }
                    }
                } else {
                    // Package list
                    div { class: "space-y-4",
                        for pkg in packages.iter() {
                            {
                                let pkg_name_for_delete = pkg.name.clone();
                                let pkg_for_raw = pkg.clone();
                                rsx! {
                                    details { class: "border border-gray-200 dark:border-gray-700 rounded bg-white dark:bg-gray-800 group",
                                        summary { class: "cursor-pointer select-none px-4 py-3 flex items-center justify-between hover:bg-gray-50 dark:hover:bg-gray-700/50",
                                            div { class: "flex items-center gap-3",
                                                span { class: "text-lg", "📦" }
                                                span { class: "font-semibold text-gray-900 dark:text-gray-100", "{pkg.name}" }
                                                span { class: "text-xs px-2 py-0.5 bg-gray-100 dark:bg-gray-700 rounded text-gray-600 dark:text-gray-400",
                                                    "{pkg.version.as_deref().unwrap_or(\"—\")}"
                                                }
                                            }
                                            div { class: "flex items-center gap-4",
                                                span { class: "text-sm text-gray-500 dark:text-gray-400",
                                                    "{pkg.classes.len()} classes • {pkg.functions.len()} functions"
                                                }
                                                // View raw button
                                                button {
                                                    class: "p-1 text-gray-400 hover:text-blue-600 dark:text-gray-500 dark:hover:text-blue-400 opacity-0 group-hover:opacity-100 transition-all",
                                                    title: "View raw data",
                                                    onclick: move |e| {
                                                        e.stop_propagation();
                                                        raw_modal_package.set(Some(pkg_for_raw.clone()));
                                                        show_raw_modal.set(true);
                                                    },
                                                    "📄"
                                                }
                                                button {
                                                    class: "p-1 text-red-600 hover:text-red-800 dark:text-red-400 dark:hover:text-red-300 opacity-0 group-hover:opacity-100 transition-opacity",
                                                    title: "Delete package",
                                                    onclick: move |e| {
                                                        e.stop_propagation();
                                                        delete_confirm.set(Some(pkg_name_for_delete.clone()));
                                                    },
                                                    "🗑️"
                                                }
                                            }
                                        }
                                        div { class: "px-4 pb-4 pt-2 space-y-4 text-sm border-t border-gray-100 dark:border-gray-700",
                                            // Dependencies
                                            if !pkg.dependencies.is_empty() {
                                                div { class: "text-gray-600 dark:text-gray-400",
                                                    span { class: "font-medium", "Dependencies:" }
                                                    span { class: "ml-2", "{pkg.dependencies.join(\", \")}" }
                                                }
                                            }

                                            // Classes
                                            if !pkg.classes.is_empty() {
                                                div {
                                                    h3 { class: "text-sm font-semibold mb-2 text-gray-800 dark:text-gray-200", "Classes" }
                                                    div { class: "space-y-2",
                                                        for class_info in pkg.classes.iter() {
                                                            {
                                                                let stateless_functions: Vec<String> = class_info.function_bindings.iter()
                                                                    .filter(|b| b.stateless)
                                                                    .map(|b| b.function_key.clone())
                                                                    .collect();
                                                                let stateful_functions: Vec<String> = class_info.function_bindings.iter()
                                                                    .filter(|b| !b.stateless)
                                                                    .map(|b| b.function_key.clone())
                                                                    .collect();
                                                                rsx! {
                                                                    div { class: "border border-gray-200 dark:border-gray-700 rounded p-3 bg-gray-50 dark:bg-gray-900",
                                                                        div { class: "flex items-center justify-between",
                                                                            span { class: "font-medium text-gray-900 dark:text-gray-100", "📋 {class_info.key}" }
                                                                            if let Some(desc) = &class_info.description {
                                                                                span { class: "text-xs text-gray-500 dark:text-gray-400 ml-2", "{desc}" }
                                                                            }
                                                                        }
                                                                        if !stateless_functions.is_empty() {
                                                                            div { class: "mt-2 text-xs text-gray-600 dark:text-gray-400",
                                                                                span { class: "font-medium text-blue-600 dark:text-blue-400", "Stateless: " }
                                                                                span { "{stateless_functions.join(\", \")}" }
                                                                            }
                                                                        }
                                                                        if !stateful_functions.is_empty() {
                                                                            div { class: "mt-1 text-xs text-gray-600 dark:text-gray-400",
                                                                                span { class: "font-medium text-purple-600 dark:text-purple-400", "Stateful: " }
                                                                                span { "{stateful_functions.join(\", \")}" }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // Functions
                                            if !pkg.functions.is_empty() {
                                                div {
                                                    h3 { class: "text-sm font-semibold mb-2 text-gray-800 dark:text-gray-200", "Functions" }
                                                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-2",
                                                        for func in pkg.functions.iter() {
                                                            div { class: "border border-gray-200 dark:border-gray-700 rounded p-2 bg-gray-50 dark:bg-gray-900",
                                                                div { class: "flex items-center justify-between",
                                                                    span { class: "font-medium text-gray-900 dark:text-gray-100", "⚡ {func.key}" }
                                                                    span { class: "text-xs px-1.5 py-0.5 rounded bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300",
                                                                        "{func.function_type:?}"
                                                                    }
                                                                }
                                                                if let Some(desc) = &func.description {
                                                                    p { class: "text-xs text-gray-600 dark:text-gray-400 mt-1", "{desc}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // Deployments
                                            if !pkg.deployments.is_empty() {
                                                div {
                                                    h3 { class: "text-sm font-semibold mb-2 text-gray-800 dark:text-gray-200", "Deployments" }
                                                    div { class: "space-y-1",
                                                        for dep in pkg.deployments.iter() {
                                                            div { class: "flex items-center gap-2 text-xs",
                                                                span { class: "text-gray-600 dark:text-gray-400", "🚀" }
                                                                span { class: "font-mono text-gray-800 dark:text-gray-200", "{dep.key}" }
                                                                span { class: "text-gray-500", "→" }
                                                                span { class: "text-gray-600 dark:text-gray-400", "{dep.class_key}" }
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
        }

        // Create Package Modal
        if show_create_modal() {
            div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                onclick: move |_| show_create_modal.set(false),
                div {
                    class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-4xl max-h-[90vh] overflow-hidden",
                    onclick: move |e| e.stop_propagation(),

                    // Modal header
                    div { class: "px-6 py-4 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center",
                        h2 { class: "text-xl font-semibold text-gray-900 dark:text-gray-100", "📦 Create Package" }
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
                                "Package YAML"
                            }
                            p { class: "text-xs text-gray-500 dark:text-gray-400 mb-2",
                                "Define your package with classes and functions. The editor validates against the OPackage schema."
                            }
                            YamlEditor {
                                value: yaml_content,
                                placeholder: "# Enter your package YAML here...",
                                height: "h-96",
                                validate_schema: true,
                                on_validation_change: move |result: ValidationResult| {
                                    yaml_valid.set(result.is_valid());
                                }
                            }
                        }

                        // Options
                        div { class: "flex items-center gap-2 mb-4",
                            input {
                                r#type: "checkbox",
                                id: "also-deploy",
                                class: "rounded border-gray-300 dark:border-gray-600",
                                checked: also_deploy(),
                                onchange: move |e| also_deploy.set(e.checked())
                            }
                            label {
                                r#for: "also-deploy",
                                class: "text-sm text-gray-700 dark:text-gray-300",
                                "Also create deployment (if deployments section included)"
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
                                title: if !yaml_valid() { "Fix validation errors first" } else { "Apply package" },
                                onclick: apply_handler,
                                if modal_loading() {
                                    span { class: "inline-block animate-spin h-4 w-4 border-2 border-white border-t-transparent rounded-full" }
                                }
                                "Apply Package"
                            }
                        }
                    }
                }
            }
        }

        // Delete Confirmation Modal
        if let Some(pkg_name) = delete_confirm() {
            div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                onclick: move |_| delete_confirm.set(None),
                div {
                    class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl p-6 max-w-md",
                    onclick: move |e| e.stop_propagation(),

                    h2 { class: "text-xl font-semibold text-gray-900 dark:text-gray-100 mb-4", "Delete Package?" }
                    p { class: "text-gray-600 dark:text-gray-400 mb-6",
                        "Are you sure you want to delete "
                        span { class: "font-mono font-semibold", "{pkg_name}" }
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
        if let Some(pkg) = raw_modal_package() {
            RawDataModal {
                show: show_raw_modal,
                json_data: to_json_string(&pkg),
                title: "Package Definition"
            }
        }
    }
}

const DEFAULT_PACKAGE_YAML: &str = r#"
name: example
version: 1.0
classes:
- description: A record service that can store and manipulate data.
  function_bindings:
  - access_modifier: PUBLIC
    function_key: Record.echo
    stateless: true
    name: echo
    parameters: []
  - access_modifier: PUBLIC
    function_key: Record.random
    stateless: false
    name: random
    parameters: []
  state_spec: 
    consistency_model: NONE
  key: Record
functions:
- key: Record.echo
  description: Echo back the provided data.
  function_type: CUSTOM
  provision_config:
    container_image: ghcr.io/pawissanutt/oaas-rs/echo-fn:latest
  config:
    HTTP_PORT: 80
    RUST_LOG: info
- key: Record.random
  description: Generate random data and store it.
  function_type: CUSTOM
  provision_config:
    container_image: ghcr.io/pawissanutt/oaas-rs/random-fn:latest
  config:
    HTTP_PORT: 80
    RUST_LOG: info
metadata:
  tags: []
dependencies: []

# Optional: include deployments to auto-deploy
deployments: 
  - key: Record
    package_name: example
    class_key: Record
    target_envs: 
      - oaas-1
      # - oaas-2
    odgm:
      log: info

"#;
