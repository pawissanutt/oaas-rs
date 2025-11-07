//! Page components

use crate::Route;
use crate::types::*;
use crate::{
    proxy_deployments, proxy_invoke, proxy_object_get, proxy_topology,
};
use dioxus::prelude::*;

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

/// Topology visualization page
#[component]
pub fn Topology() -> Element {
    let mut topology = use_signal(|| None::<crate::types::TopologySnapshot>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Auto-fetch on mount
    use_effect(move || {
        spawn(async move {
            loading.set(true);
            match proxy_topology().await {
                Ok(data) => topology.set(Some(data)),
                Err(e) => error_msg.set(Some(format!("Error: {}", e))),
            }
            loading.set(false);
        });
    });

    let node_icon = |node_type: &str| match node_type {
        "gateway" => "🌐",
        "router" => "📡",
        "odgm" => "💾",
        "function" => "⚡",
        _ => "📦",
    };

    let status_color = |status: &str| match status {
        "healthy" => {
            "bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300 border-green-200 dark:border-green-800"
        }
        "degraded" => {
            "bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300 border-yellow-200 dark:border-yellow-800"
        }
        "down" => {
            "bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300 border-red-200 dark:border-red-800"
        }
        _ => {
            "bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300 border-gray-200 dark:border-gray-600"
        }
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
                    // Info Bar
                    div { class: "bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 text-blue-800 dark:text-blue-300 px-4 py-3 rounded mb-6",
                        "Showing {topo.nodes.len()} nodes and {topo.edges.len()} connections • Last updated: {topo.timestamp}"
                    }

                    // Nodes Grid
                    div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 mb-6",
                        for node in topo.nodes.iter() {
                            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-4 border-2 {status_color(&node.status)}",
                                div { class: "flex items-center mb-3",
                                    span { class: "text-2xl mr-2", "{node_icon(&node.node_type)}" }
                                    div {
                                        h3 { class: "font-semibold text-gray-800 dark:text-gray-100", "{node.id}" }
                                        p { class: "text-xs text-gray-600 dark:text-gray-400 capitalize", "{node.node_type}" }
                                    }
                                }

                                if !node.metadata.is_empty() {
                                    div { class: "space-y-1",
                                        for (key, value) in node.metadata.iter() {
                                            div { class: "flex text-xs",
                                                span { class: "text-gray-600 dark:text-gray-400 w-20", "{key}:" }
                                                span { class: "font-mono text-gray-800 dark:text-gray-100", "{value}" }
                                            }
                                        }
                                    }
                                }

                                div { class: "mt-3 pt-3 border-t border-gray-200 dark:border-gray-700",
                                    span { class: "text-xs font-semibold uppercase {status_color(&node.status)} px-2 py-1 rounded",
                                        "{node.status}"
                                    }
                                }
                            }
                        }
                    }

                    // Connections
                    div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6",
                        h2 { class: "text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100", "Network Connections" }
                        div { class: "space-y-2",
                            for (from, to) in topo.edges.iter() {
                                div { class: "flex items-center text-sm",
                                    span { class: "font-mono text-blue-600 dark:text-blue-400", "{from}" }
                                    span { class: "mx-3 text-gray-400 dark:text-gray-500", "→" }
                                    span { class: "font-mono text-blue-600 dark:text-blue-400", "{to}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
