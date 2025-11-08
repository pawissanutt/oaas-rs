//! Page components

use crate::Route;
use crate::types::*;
use crate::{
    proxy_deployments, proxy_invoke, proxy_object_get, proxy_packages,
    proxy_topology,
};
use dioxus::prelude::*;

// JavaScript interop for Cytoscape (client-side only)
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = initCytoscapeGraph, catch)]
    fn js_init_cytoscape_graph(
        container_id: &str,
        nodes: JsValue,
        edges: JsValue,
    ) -> Result<JsValue, JsValue>;
}

/// Initialize Cytoscape graph with topology data
#[cfg(target_arch = "wasm32")]
fn init_cytoscape_graph(topology: TopologySnapshot) {
    use wasm_bindgen::JsValue;

    // Convert nodes and edges to JsValue
    let nodes_js = match serde_wasm_bindgen::to_value(&topology.nodes) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to serialize nodes: {:?}", e);
            return;
        }
    };

    let edges_js = match serde_wasm_bindgen::to_value(&topology.edges) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to serialize edges: {:?}", e);
            return;
        }
    };

    // Call JavaScript function with error handling
    match js_init_cytoscape_graph("cy-container", nodes_js, edges_js) {
        Ok(_) => tracing::info!("Cytoscape graph initialized successfully"),
        Err(e) => tracing::error!("Failed to initialize Cytoscape: {:?}", e),
    }
}

/// Register node selection callback (must be called before graph init)
#[cfg(target_arch = "wasm32")]
fn register_node_callback(mut callback: impl FnMut(Option<String>) + 'static) {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::*;

    let closure = Closure::wrap(Box::new(move |node_id: JsValue| {
        if node_id.is_null() || node_id.is_undefined() {
            callback(None);
        } else if let Some(id_str) = node_id.as_string() {
            callback(Some(id_str));
        }
    }) as Box<dyn FnMut(JsValue)>);

    if let Some(window) = web_sys::window() {
        // Set the callback
        let _ = js_sys::Reflect::set(
            &window,
            &JsValue::from_str("cytoscapeNodeSelectCallback"),
            closure.as_ref().unchecked_ref(),
        );

        // Verify it was set
        tracing::info!("Callback registered on window");
    }

    closure.forget(); // Keep callback alive
}

// // Stub for non-WASM targets (server-side)
// #[cfg(not(target_arch = "wasm32"))]
// fn init_cytoscape_graph(_topology: TopologySnapshot) {
//     // No-op on server side
// }

// #[cfg(not(target_arch = "wasm32"))]
// fn register_node_callback<F>(_callback: F)
// where
//     F: FnMut(Option<String>) + 'static,
// {
//     // No-op on server side
// }

/// Home page with quick links
#[component]
pub fn Home() -> Element {
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
                        li {
                            Link {
                                to: Route::Invoke {},
                                class: "text-blue-600 dark:text-blue-400 hover:underline",
                                "Invoke Functions"
                            }
                        }
                        li {
                            Link {
                                to: Route::Objects {},
                                class: "text-blue-600 dark:text-blue-400 hover:underline",
                                "Browse Objects"
                            }
                        }
                        li {
                            Link {
                                to: Route::Deployments {},
                                class: "text-blue-600 dark:text-blue-400 hover:underline",
                                "View Deployments"
                            }
                        }
                        li {
                            Link {
                                to: Route::Topology {},
                                class: "text-blue-600 dark:text-blue-400 hover:underline",
                                "Topology Graph"
                            }
                        }
                    }
                }
                div { class: "border border-gray-200 dark:border-gray-700 rounded p-4 bg-white dark:bg-gray-800",
                    h2 { class: "text-xl font-semibold mb-2 text-gray-900 dark:text-gray-100", "System Status" }
                    p { class: "text-sm text-gray-500 dark:text-gray-400", "Status checks coming soon..." }
                }
            }
        }
    }
}

/// Packages listing page (grouped by package)
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
                                    // Classes
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
                                    // Functions (top-level)
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

/// Function invocation page
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

/// Object browser page
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
                                    if let Some(ref str_id) = meta.object_id_str {
                                        "{str_id}"
                                    } else {
                                        "{meta.object_id}"
                                    }
                                }
                            }
                        }
                    }

                    // Entries
                    if !obj.entries.is_empty() || !obj.entries_str.is_empty() {
                        div {
                            h3 { class: "text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase mb-3", "Entries" }
                            div { class: "space-y-2",
                                for (key, val) in obj.entries_str.iter() {
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
                                for (key, val) in obj.entries_str.iter() {
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

/// Deployments management page
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

/// Topology visualization page with Cytoscape.js
#[component]
pub fn Topology() -> Element {
    let mut topology = use_signal(|| None::<crate::types::TopologySnapshot>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let selected_node_id = use_signal(|| None::<String>);
    let mut show_panel = use_signal(|| true);

    // Auto-fetch topology data on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            match proxy_topology().await {
                Ok(data) => {
                    topology.set(Some(data));
                }
                Err(e) => error_msg.set(Some(format!("Error: {}", e))),
            }
            loading.set(false);
        });
    });

    // Get selected node details
    let selected_node = move || {
        selected_node_id().and_then(|id| {
            topology().and_then(|topo| {
                topo.nodes.iter().find(|n| n.id == id).cloned()
            })
        })
    };

    rsx! {
        div { class: "container mx-auto p-6",
            h1 { class: "text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100", "Topology View" }

            if loading() {
                div { class: "text-center py-12",
                    div { class: "inline-block animate-spin rounded-full h-8 w-8 border-b-2 border-blue-600 dark:border-blue-400" }
                    p { class: "mt-2 text-gray-600 dark:text-gray-400", "Loading topology..." }
                }
            } else if let Some(err) = error_msg() {
                div { class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded",
                    "{err}"
                }
            } else if let Some(topo) = topology() {
                div {
                    // Info Bar with Panel Toggle
                    div { class: "bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 text-blue-800 dark:text-blue-300 px-4 py-3 rounded mb-4 flex items-center justify-between",
                        span {
                            "Showing {topo.nodes.len()} nodes and {topo.edges.len()} connections • Last updated: {topo.timestamp}"
                        }
                        div { class: "flex items-center gap-4",
                            div { class: "text-sm",
                                span { class: "inline-block w-3 h-3 rounded-full bg-blue-500 mr-1" }
                                "Gateway  "
                                span { class: "inline-block w-3 h-3 rounded-full bg-purple-500 mr-1 ml-3" }
                                "Router  "
                                span { class: "inline-block w-3 h-3 rounded-full bg-green-500 mr-1 ml-3" }
                                "ODGM  "
                                span { class: "inline-block w-3 h-3 rounded-full bg-orange-500 mr-1 ml-3" }
                                "Function"
                            }
                            button {
                                class: "px-3 py-1 text-sm bg-blue-600 dark:bg-blue-500 text-white rounded hover:bg-blue-700 dark:hover:bg-blue-600 transition-colors",
                                onclick: move |_| show_panel.set(!show_panel()),
                                if show_panel() { "Hide Panel" } else { "Show Panel" }
                            }
                        }
                    }

                    // Main Content: Graph + Slide-over Details Panel
                    div { class: "relative",
                        // Interactive Graph Container (full-width; panel overlays)
                        div {
                            id: "cy-container",
                            class: "w-full bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700",
                            style: "height: 600px;",
                            onmounted: move |_| {
                                tracing::info!("Graph container mounted");

                                // Register callback first, then initialize graph
                                #[cfg(target_arch = "wasm32")]
                                {
                                    tracing::info!("Registering node callback");
                                    let mut node_sig = selected_node_id; // Signal is Copy
                                    register_node_callback(move |node_id| {
                                        tracing::info!("Callback called with: {:?}", node_id);
                                        node_sig.set(node_id);
                                    });
                                }

                                // Initialize graph after container is mounted (WASM only)
                                #[cfg(target_arch = "wasm32")]
                                {
                                    if let Some(topo_data) = topology() {
                                        spawn(async move {
                                            // Small delay to ensure DOM is fully ready
                                            gloo_timers::future::TimeoutFuture::new(50).await;
                                            init_cytoscape_graph(topo_data);
                                        });
                                    }
                                }
                            },
                            // Graph renders here via JS
                        }

                        // Details Panel (conditionally rendered: simply appears/disappears)
                        if show_panel() {
                            div {
                                class: "absolute top-0 right-0 h-[600px] w-80 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 shadow-lg rounded-l-lg p-4 z-20",
                                style: "overflow-y: auto;",
                                if let Some(node) = selected_node() {
                                    // Node Details
                                    div {
                                        h3 { class: "text-lg font-bold mb-3 text-gray-900 dark:text-gray-100 border-b pb-2 dark:border-gray-700",
                                            "{node.id}"
                                        }

                                        // Node Type
                                        div { class: "mb-4",
                                            div { class: "text-xs text-gray-500 dark:text-gray-400 uppercase mb-1", "Type" }
                                            div { class: "flex items-center gap-2",
                                                span {
                                                    class: match node.node_type.as_str() {
                                                        "gateway" => "px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-300 rounded text-sm",
                                                        "router" => "px-2 py-1 bg-purple-100 dark:bg-purple-900/30 text-purple-800 dark:text-purple-300 rounded text-sm",
                                                        "odgm" => "px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300 rounded text-sm",
                                                        "function" => "px-2 py-1 bg-orange-100 dark:bg-orange-900/30 text-orange-800 dark:text-orange-300 rounded text-sm",
                                                        _ => "px-2 py-1 bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300 rounded text-sm",
                                                    },
                                                    "{node.node_type}"
                                                }
                                            }
                                        }

                                        // Status
                                        div { class: "mb-4",
                                            div { class: "text-xs text-gray-500 dark:text-gray-400 uppercase mb-1", "Status" }
                                            span {
                                                class: match node.status.as_str() {
                                                    "healthy" => "px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300 rounded text-sm font-semibold",
                                                    "degraded" => "px-2 py-1 bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300 rounded text-sm font-semibold",
                                                    "down" => "px-2 py-1 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300 rounded text-sm font-semibold",
                                                    _ => "px-2 py-1 bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300 rounded text-sm font-semibold",
                                                },
                                                "{node.status}"
                                            }
                                        }

                                        // Metadata
                                        if !node.metadata.is_empty() {
                                            div { class: "mb-4",
                                                div { class: "text-xs text-gray-500 dark:text-gray-400 uppercase mb-2", "Metadata" }
                                                div { class: "space-y-1",
                                                    for (key, value) in node.metadata.iter() {
                                                        div { class: "flex justify-between text-sm",
                                                            span { class: "text-gray-600 dark:text-gray-400", "{key}:" }
                                                            span { class: "font-mono text-gray-900 dark:text-gray-100", "{value}" }
                                                        }
                                                    }
                                                }
                                            }
                                        }

                                        // Deployed Classes
                                        if !node.deployed_classes.is_empty() {
                                            div { class: "mb-4",
                                                div { class: "text-xs text-gray-500 dark:text-gray-400 uppercase mb-2", "Deployed Classes" }
                                                div { class: "space-y-2",
                                                    for class_name in node.deployed_classes.iter() {
                                                        div { class: "bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded p-2",
                                                            div { class: "font-semibold text-sm text-gray-900 dark:text-gray-100 mb-1",
                                                                "{class_name}"
                                                            }
                                                            div { class: "text-xs text-gray-600 dark:text-gray-400 space-y-1",
                                                                div { "• Functions: echo, process" }
                                                                div { "• Attributes: state, counter" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    // No selection placeholder (only when panel visible)
                                    div { class: "flex flex-col items-center justify-center h-full text-gray-400 dark:text-gray-500",
                                        div { class: "text-4xl mb-3", "👆" }
                                        div { class: "text-center text-sm",
                                            "Click on a node in the graph"
                                            br {}
                                            "to view details"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Controls hint
                    div { class: "mt-4 text-sm text-gray-600 dark:text-gray-400 text-center",
                        "💡 Click and drag nodes • Scroll to zoom • Click nodes to view details"
                    }
                }
            }
        }
    }
}
