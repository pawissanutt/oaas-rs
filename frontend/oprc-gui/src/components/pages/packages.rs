//! Packages page component

use dioxus::prelude::*;
use crate::{proxy_packages};
use crate::types::*;

#[component]
pub fn Packages() -> Element {
    let mut loading = use_signal(|| true);
    let mut error_msg = use_signal(|| None::<String>);
    let mut snapshot = use_signal(|| None::<PackagesSnapshot>);

    use_effect(move || {
        spawn(async move {
            match proxy_packages().await {
                Ok(data) => {
                    snapshot.set(Some(data));
                    loading.set(false);
                }
                Err(e) => {
                    error_msg.set(Some(e.to_string()));
                    loading.set(false);
                }
            }
        });
    });

    rsx! {
        div { class: "container mx-auto p-6",
            h1 { class: "text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100", "Packages" }
            if loading() {
                div { class: "text-center py-12",
                    div { class: "inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 dark:border-blue-400" }
                    p { class: "mt-2 text-gray-600 dark:text-gray-400", "Loading packages..." }
                }
            } else if let Some(err) = error_msg() {
                div { class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded", "{err}" }
            } else if let Some(data) = snapshot() {
                if data.packages.is_empty() {
                    div { class: "text-gray-600 dark:text-gray-400", "No packages available." }
                } else {
                    div { class: "space-y-4",
                        for pkg in data.packages.iter() {
                            details { class: "border border-gray-200 dark:border-gray-700 rounded bg-white dark:bg-gray-800",
                                summary { class: "cursor-pointer select-none px-4 py-2 flex items-center justify-between",
                                    span { class: "font-semibold text-gray-900 dark:text-gray-100", "{pkg.name}" }
                                    span { class: "text-xs text-gray-500 dark:text-gray-400", "{pkg.version.as_deref().unwrap_or(\"unknown\")}" }
                                }
                                div { class: "px-4 pb-4 pt-2 space-y-4 text-sm",
                                    if !pkg.dependencies.is_empty() {
                                        div { class: "text-gray-600 dark:text-gray-400",
                                            span { class: "font-medium", "Dependencies:" }
                                            span { class: "ml-2", "{pkg.dependencies.join(\", \")}" }
                                        }
                                    }
                                    if !pkg.classes.is_empty() {
                                        div {
                                            h3 { class: "text-sm font-semibold mb-1 text-gray-800 dark:text-gray-200", "Classes" }
                                            div { class: "space-y-2",
                                                for class_info in pkg.classes.iter() {
                                                    div { class: "border border-gray-200 dark:border-gray-700 rounded p-2 bg-gray-50 dark:bg-gray-900",
                                                        div { class: "flex items-center justify-between",
                                                            span { class: "font-medium text-gray-900 dark:text-gray-100", "{class_info.key}" }
                                                            if let Some(desc) = &class_info.description { span { class: "text-xs text-gray-500 dark:text-gray-400", "{desc}" } }
                                                        }
                                                        if !class_info.stateless_functions.is_empty() {
                                                            div { class: "mt-1 text-xs text-gray-600 dark:text-gray-400",
                                                                span { class: "font-medium", "Stateless:" }
                                                                span { class: "ml-1", "{class_info.stateless_functions.join(\", \")}" }
                                                            }
                                                        }
                                                        if !class_info.stateful_functions.is_empty() {
                                                            div { class: "mt-1 text-xs text-gray-600 dark:text-gray-400",
                                                                span { class: "font-medium", "Stateful:" }
                                                                span { class: "ml-1", "{class_info.stateful_functions.join(\", \")}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if !pkg.functions.is_empty() {
                                        div {
                                            h3 { class: "text-sm font-semibold mb-1 text-gray-800 dark:text-gray-200", "Functions" }
                                            div { class: "grid grid-cols-1 md:grid-cols-2 gap-2",
                                                for func in pkg.functions.iter() {
                                                    div { class: "border border-gray-200 dark:border-gray-700 rounded p-2 bg-gray-50 dark:bg-gray-900 flex flex-col",
                                                        span { class: "font-medium text-gray-900 dark:text-gray-100", "{func.key}" }
                                                        span { class: "text-xs text-gray-500 dark:text-gray-400", "Type: {func.function_type}" }
                                                        if let Some(desc) = &func.description { span { class: "text-xs text-gray-600 dark:text-gray-400", "{desc}" } }
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
