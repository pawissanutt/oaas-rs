//! Invoke page component (classic form)

use crate::proxy_invoke;
use crate::types::*;
use dioxus::prelude::*;

#[component]
pub fn Invoke() -> Element {
    let mut class_key = use_signal(|| "EchoClass".to_string());
    let mut partition_id = use_signal(|| "0".to_string());
    let mut function_key = use_signal(|| "echo".to_string());
    let mut object_id = use_signal(|| "".to_string());
    let mut payload =
        use_signal(|| r#"{"message": "Hello, OaaS!"}"#.to_string());
    let mut response = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);

    let invoke_handler = move |_| {
        spawn(async move {
            loading.set(true);
            response.set(None);

            let req = InvokeRequest {
                class_key: class_key(),
                partition_id: partition_id(),
                function_key: function_key(),
                payload: serde_json::from_str(&payload())
                    .unwrap_or(serde_json::json!({})),
                object_id: if object_id().is_empty() {
                    None
                } else {
                    Some(object_id())
                },
            };

            match proxy_invoke(req).await {
                Ok(resp) => {
                    let result = if let Some(ref payload) = resp.payload {
                        String::from_utf8_lossy(payload).to_string()
                    } else {
                        "Empty response".to_string()
                    };
                    response.set(Some(format!(
                        "✓ Success\nStatus: {}\n\nResponse:\n{}",
                        resp.status, result
                    )));
                }
                Err(e) => {
                    response.set(Some(format!("✗ Error: {}", e)));
                }
            }

            loading.set(false);
        });
    };

    rsx! {
        div { class: "container mx-auto p-6 max-w-4xl",
            h1 { class: "text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100", "Function Invocation" }

            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6 space-y-4",
                // Class Key
                div {
                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                        "Class Key"
                    }
                    input {
                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                        r#type: "text",
                        value: "{class_key}",
                        oninput: move |e| class_key.set(e.value()),
                        placeholder: "e.g., EchoClass"
                    }
                }

                // Partition ID
                div {
                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                        "Partition ID"
                    }
                    input {
                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                        r#type: "text",
                        value: "{partition_id}",
                        oninput: move |e| partition_id.set(e.value()),
                        placeholder: "0"
                    }
                }

                // Function Key
                div {
                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                        "Function Key"
                    }
                    input {
                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                        r#type: "text",
                        value: "{function_key}",
                        oninput: move |e| function_key.set(e.value()),
                        placeholder: "e.g., echo"
                    }
                }

                // Object ID (optional)
                div {
                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                        "Object ID "
                        span { class: "text-gray-400 dark:text-gray-500 text-xs", "(optional, for stateful invocation)" }
                    }
                    input {
                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                        r#type: "text",
                        value: "{object_id}",
                        oninput: move |e| object_id.set(e.value()),
                        placeholder: "Leave empty for stateless invocation"
                    }
                }

                // Payload
                div {
                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                        "Payload (JSON)"
                    }
                    textarea {
                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 dark:focus:ring-blue-400 font-mono text-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                        rows: 6,
                        value: "{payload}",
                        oninput: move |e| payload.set(e.value())
                    }
                }

                // Invoke Button
                button {
                    class: "w-full bg-blue-600 dark:bg-blue-500 text-white py-2 px-4 rounded-md hover:bg-blue-700 dark:hover:bg-blue-600 disabled:bg-gray-400 dark:disabled:bg-gray-600 disabled:cursor-not-allowed transition-colors",
                    disabled: loading(),
                    onclick: invoke_handler,
                    if loading() {
                        "Invoking..."
                    } else {
                        "Invoke Function"
                    }
                }

                // Response Display
                if let Some(resp) = response() {
                    div { class: "mt-4 p-4 bg-gray-50 dark:bg-gray-900 rounded-md border border-gray-200 dark:border-gray-700",
                        pre { class: "text-sm whitespace-pre-wrap font-mono text-gray-900 dark:text-gray-100",
                            "{resp}"
                        }
                    }
                }
            }
        }
    }
}
