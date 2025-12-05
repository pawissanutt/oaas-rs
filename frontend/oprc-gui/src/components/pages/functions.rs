//! Functions page component - browse and filter all functions across packages

use crate::components::{to_json_string, RawDataModal};
use crate::proxy_packages;
use dioxus::prelude::*;
use oprc_models::{FunctionType, OFunction};

/// A function with its source package info for display
#[derive(Debug, Clone)]
struct FunctionInfo {
    pub function: OFunction,
    pub package_name: String,
    pub package_version: Option<String>,
    /// Classes that bind this function
    pub bound_to_classes: Vec<BoundClass>,
}

#[derive(Debug, Clone)]
struct BoundClass {
    pub class_key: String,
    pub binding_name: String,
    pub stateless: bool,
}

#[component]
pub fn Functions() -> Element {
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut functions = use_signal(|| Vec::<FunctionInfo>::new());
    let mut search_filter = use_signal(|| "".to_string());
    let mut type_filter = use_signal(|| "all".to_string());

    // Raw data modal state
    let mut show_raw_modal = use_signal(|| false);
    let mut raw_modal_function = use_signal(|| None::<OFunction>);

    // Load functions from all packages
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);

            match proxy_packages().await {
                Ok(data) => {
                    let mut all_functions = Vec::new();

                    for pkg in data.packages {
                        for func in &pkg.functions {
                            // Find which classes bind this function
                            let bound_classes: Vec<BoundClass> = pkg
                                .classes
                                .iter()
                                .flat_map(|cls| {
                                    cls.function_bindings
                                        .iter()
                                        .filter(|b| b.function_key == func.key)
                                        .map(|b| BoundClass {
                                            class_key: cls.key.clone(),
                                            binding_name: b.name.clone(),
                                            stateless: b.stateless,
                                        })
                                })
                                .collect();

                            all_functions.push(FunctionInfo {
                                function: func.clone(),
                                package_name: pkg.name.clone(),
                                package_version: pkg.version.clone(),
                                bound_to_classes: bound_classes,
                            });
                        }
                    }

                    functions.set(all_functions);
                }
                Err(e) => {
                    error_msg.set(Some(e.to_string()));
                }
            }
            loading.set(false);
        });
    });

    // Filter functions
    let filtered_functions = move || {
        let search = search_filter().to_lowercase();
        let type_f = type_filter();

        functions()
            .into_iter()
            .filter(|f| {
                // Type filter
                if type_f != "all" {
                    let matches_type = match type_f.as_str() {
                        "builtin" => f.function.function_type == FunctionType::Builtin,
                        "custom" => f.function.function_type == FunctionType::Custom,
                        "macro" => f.function.function_type == FunctionType::Macro,
                        "logical" => f.function.function_type == FunctionType::Logical,
                        _ => true,
                    };
                    if !matches_type {
                        return false;
                    }
                }

                // Search filter
                if !search.is_empty() {
                    let matches_search = f.function.key.to_lowercase().contains(&search)
                        || f.package_name.to_lowercase().contains(&search)
                        || f.function
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&search))
                            .unwrap_or(false);
                    if !matches_search {
                        return false;
                    }
                }

                true
            })
            .collect::<Vec<_>>()
    };

    let function_type_badge = |ft: &FunctionType| {
        let (color, text) = match ft {
            FunctionType::Builtin => (
                "bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300",
                "Builtin",
            ),
            FunctionType::Custom => (
                "bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-300",
                "Custom",
            ),
            FunctionType::Macro => (
                "bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300",
                "Macro",
            ),
            FunctionType::Logical => (
                "bg-purple-100 dark:bg-purple-900/30 text-purple-800 dark:text-purple-300",
                "Logical",
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
            // Header
            div { class: "flex justify-between items-center mb-6",
                h1 { class: "text-2xl font-bold text-gray-900 dark:text-gray-100", "⚡ Functions" }
            }

            // Filters
            div { class: "flex flex-wrap gap-4 mb-6",
                // Search
                input {
                    class: "flex-1 min-w-[200px] md:max-w-sm px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                    r#type: "text",
                    placeholder: "🔍 Search functions...",
                    value: "{search_filter}",
                    oninput: move |e| search_filter.set(e.value())
                }
                // Type filter
                select {
                    class: "px-4 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                    value: "{type_filter}",
                    onchange: move |e| type_filter.set(e.value()),
                    option { value: "all", "All Types" }
                    option { value: "custom", "Custom" }
                    option { value: "builtin", "Builtin" }
                    option { value: "macro", "Macro" }
                    option { value: "logical", "Logical" }
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
                    p { class: "mt-2 text-gray-600 dark:text-gray-400", "Loading functions..." }
                }
            } else {
                {
                    let funcs = filtered_functions();
                    if funcs.is_empty() {
                        rsx! {
                            div { class: "text-center py-12 bg-white dark:bg-gray-800 rounded-lg shadow",
                                p { class: "text-gray-500 dark:text-gray-400", "No functions found" }
                            }
                        }
                    } else {
                        rsx! {
                            // Stats
                            div { class: "mb-4 text-sm text-gray-600 dark:text-gray-400",
                                "{funcs.len()} function(s) found"
                            }

                            // Function cards
                            div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
                                for info in funcs.iter() {
                                    {
                                        let func_for_raw = info.function.clone();
                                        rsx! {
                                            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-4 hover:shadow-lg transition group",
                                                // Header
                                                div { class: "flex justify-between items-start mb-3",
                                                    div { class: "flex-1",
                                                        h3 { class: "font-semibold text-gray-900 dark:text-gray-100", "⚡ {info.function.key}" }
                                                        p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1",
                                                            "Package: "
                                                            span { class: "font-mono", "{info.package_name}" }
                                                            if let Some(ver) = &info.package_version {
                                                                " v{ver}"
                                                            }
                                                        }
                                                    }
                                                    div { class: "flex items-center gap-2",
                                                        {function_type_badge(&info.function.function_type)}
                                                        // View raw button
                                                        button {
                                                            class: "p-1 text-gray-400 hover:text-blue-600 dark:text-gray-500 dark:hover:text-blue-400 opacity-0 group-hover:opacity-100 transition-all",
                                                            title: "View raw data",
                                                            onclick: move |_| {
                                                                raw_modal_function.set(Some(func_for_raw.clone()));
                                                                show_raw_modal.set(true);
                                                            },
                                                            "📄"
                                                        }
                                                    }
                                                }

                                                // Description
                                                if let Some(desc) = &info.function.description {
                                                    p { class: "text-sm text-gray-600 dark:text-gray-400 mb-3", "{desc}" }
                                                }

                                                // Provision config
                                                if let Some(config) = &info.function.provision_config {
                                                    div { class: "bg-gray-50 dark:bg-gray-900 rounded p-3 mb-3 text-xs",
                                                        if let Some(image) = &config.container_image {
                                                            div { class: "flex items-center gap-2 mb-1",
                                                                span { class: "text-gray-500 dark:text-gray-400", "Image:" }
                                                                span { class: "font-mono text-gray-800 dark:text-gray-200 truncate", "{image}" }
                                                            }
                                                        }
                                                        if let Some(port) = config.port {
                                                            div { class: "flex items-center gap-2",
                                                                span { class: "text-gray-500 dark:text-gray-400", "Port:" }
                                                                span { class: "font-mono text-gray-800 dark:text-gray-200", "{port}" }
                                                            }
                                                        }
                                                        if let Some(min) = config.min_scale {
                                                            div { class: "flex items-center gap-2",
                                                                span { class: "text-gray-500 dark:text-gray-400", "Min scale:" }
                                                                span { class: "font-mono text-gray-800 dark:text-gray-200", "{min}" }
                                                            }
                                                        }
                                                        if let Some(max) = config.max_scale {
                                                            div { class: "flex items-center gap-2",
                                                                span { class: "text-gray-500 dark:text-gray-400", "Max scale:" }
                                                                span { class: "font-mono text-gray-800 dark:text-gray-200", "{max}" }
                                                            }
                                                        }
                                                    }
                                                }

                                                // Bound classes
                                                if !info.bound_to_classes.is_empty() {
                                                    div { class: "border-t border-gray-200 dark:border-gray-700 pt-3 mt-3",
                                                        p { class: "text-xs text-gray-500 dark:text-gray-400 mb-2", "Bound to classes:" }
                                                        div { class: "flex flex-wrap gap-1",
                                                            for cls in info.bound_to_classes.iter() {
                                                                span {
                                                                    class: "inline-flex items-center px-2 py-0.5 text-xs rounded",
                                                                    class: if cls.stateless {
                                                                        "bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300"
                                                                    } else {
                                                                        "bg-purple-100 dark:bg-purple-900/30 text-purple-700 dark:text-purple-300"
                                                                    },
                                                                    "{cls.class_key}"
                                                                    span { class: "text-gray-500 dark:text-gray-400 ml-1", ".{cls.binding_name}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }

                                                // Config entries (if any)
                                                if !info.function.config.is_empty() {
                                                    div { class: "border-t border-gray-200 dark:border-gray-700 pt-3 mt-3",
                                                        p { class: "text-xs text-gray-500 dark:text-gray-400 mb-2", "Config:" }
                                                        div { class: "space-y-1",
                                                            for (k, v) in info.function.config.iter() {
                                                                div { class: "text-xs",
                                                                    span { class: "font-mono text-gray-600 dark:text-gray-400", "{k}" }
                                                                    span { class: "text-gray-500", "=" }
                                                                    span { class: "font-mono text-gray-800 dark:text-gray-200", "{v}" }
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
        }

        // Raw Data Modal
        if let Some(func) = raw_modal_function() {
            RawDataModal {
                show: show_raw_modal,
                json_data: to_json_string(&func),
                title: "Function Definition"
            }
        }
    }
}
