//! Objects page component (read-only browser)

use crate::proxy_object_get;
use crate::types::*;
use dioxus::prelude::*;

#[component]
pub fn Objects() -> Element {
    let mut class_key = use_signal(|| "Counter".to_string());
    let mut partition_id = use_signal(|| "0".to_string());
    let mut object_id = use_signal(|| "user:123".to_string());
    let mut object_data = use_signal(|| None::<crate::types::ObjData>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    let fetch_handler = move |_| {
        spawn(async move {
            loading.set(true);
            error_msg.set(None);
            object_data.set(None);

            let req = ObjectGetRequest {
                class_key: class_key(),
                partition_id: partition_id(),
                object_id: object_id(),
            };

            match proxy_object_get(req).await {
                Ok(data) => {
                    object_data.set(Some(data));
                }
                Err(e) => {
                    error_msg.set(Some(format!("Error: {}", e)));
                }
            }

            loading.set(false);
        });
    };

    rsx! {
        div { class: "container mx-auto p-6 max-w-4xl",
            h1 { class: "text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100", "Object Browser" }

            // Fetch Form
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6 mb-6",
                div { class: "grid grid-cols-1 md:grid-cols-3 gap-4 mb-4",
                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Class Key"
                        }
                        input {
                            class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            r#type: "text",
                            value: "{class_key}",
                            oninput: move |e| class_key.set(e.value())
                        }
                    }

                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Partition ID"
                        }
                        input {
                            class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            r#type: "text",
                            value: "{partition_id}",
                            oninput: move |e| partition_id.set(e.value())
                        }
                    }

                    div {
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Object ID"
                        }
                        input {
                            class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            r#type: "text",
                            value: "{object_id}",
                            oninput: move |e| object_id.set(e.value())
                        }
                    }
                }

                button {
                    class: "w-full bg-blue-600 dark:bg-blue-500 text-white py-2 px-4 rounded-md hover:bg-blue-700 dark:hover:bg-blue-600 disabled:bg-gray-400 dark:disabled:bg-gray-600 transition-colors",
                    disabled: loading(),
                    onclick: fetch_handler,
                    if loading() { "Fetching..." } else { "Fetch Object" }
                }
            }

            // Error Display
            if let Some(err) = error_msg() {
                div { class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded mb-6",
                    "{err}"
                }
            }

            // Object Display
            if let Some(obj) = object_data() {
                div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6",
                    h2 { class: "text-xl font-semibold mb-4 text-gray-900 dark:text-gray-100", "Object Data" }

                    // Metadata
                    if let Some(meta) = &obj.metadata {
                        div { class: "mb-6 pb-4 border-b border-gray-200 dark:border-gray-700",
                            h3 { class: "text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase mb-2", "Metadata" }
                            dl { class: "grid grid-cols-2 gap-2 text-sm",
                                dt { class: "text-gray-600 dark:text-gray-400", "Class:" }
                                dd { class: "font-mono text-gray-900 dark:text-gray-100", "{meta.cls_id}" }
                                dt { class: "text-gray-600 dark:text-gray-400", "Partition:" }
                                dd { class: "font-mono text-gray-900 dark:text-gray-100", "{meta.partition_id}" }
                                dt { class: "text-gray-600 dark:text-gray-400", "Object ID:" }
                                dd { class: "font-mono text-gray-900 dark:text-gray-100",
                                    if let Some(ref id) = meta.object_id {
                                        "{id}"
                                    } else {
                                        "-"
                                    }
                                }
                            }
                        }
                    }

                    // Entries
                    if !obj.entries.is_empty() {
                        div {
                            h3 { class: "text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase mb-3", "Entries" }
                            div { class: "space-y-2",
                                for (key, val) in obj.entries.iter() {
                                    div { class: "bg-gray-50 dark:bg-gray-900 p-3 rounded border border-gray-200 dark:border-gray-700",
                                        div { class: "flex justify-between items-start mb-1",
                                            span { class: "font-mono text-sm text-green-600 dark:text-green-400", "\"{key}\"" }
                                            span { class: "text-xs text-gray-500 dark:text-gray-400", "Type: {val.r#type}" }
                                        }
                                        pre { class: "text-xs font-mono text-gray-700 dark:text-gray-300 whitespace-pre-wrap mt-2",
                                            {String::from_utf8_lossy(&val.data).to_string()}
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        p { class: "text-gray-500 dark:text-gray-400 text-sm", "No entries in this object" }
                    }
                }
            }
        }
    }
}
