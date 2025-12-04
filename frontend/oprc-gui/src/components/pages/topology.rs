//! Topology page component with Cytoscape WASM interop

use crate::proxy_topology;
use crate::types::*;
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
    let nodes_js = match serde_wasm_bindgen::to_value(&topology.nodes) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(?e, "Failed to serialize nodes");
            return;
        }
    };
    let edges_js = match serde_wasm_bindgen::to_value(&topology.edges) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!(?e, "Failed to serialize edges");
            return;
        }
    };

    if let Err(e) = js_init_cytoscape_graph("cy-container", nodes_js, edges_js)
    {
        tracing::error!(?e, "Failed to initialize Cytoscape");
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
        let _ = js_sys::Reflect::set(
            &window,
            &JsValue::from_str("cytoscapeNodeSelectCallback"),
            closure.as_ref().unchecked_ref(),
        );
    }

    closure.forget();
}

fn format_timestamp(
    ts: Option<&oprc_grpc::proto::common::Timestamp>,
) -> String {
    ts.map(|t| format!("{}", t.seconds)).unwrap_or_default()
}

#[component]
pub fn Topology() -> Element {
    let mut topology_mode = use_signal(|| TopologySource::Deployments);
    let mut topology = use_signal(|| None::<crate::types::TopologySnapshot>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);
    let mut selected_node_id = use_signal(|| None::<String>);
    let mut show_panel = use_signal(|| true);

    use_effect(move || {
        let mode = topology_mode();
        spawn(async move {
            loading.set(true);
            error_msg.set(None);
            selected_node_id.set(None);
            topology.set(None);

            let result = {
                let request = TopologyRequest { source: mode };
                proxy_topology(request).await
            };

            match result {
                Ok(data) => {
                    #[cfg(target_arch = "wasm32")]
                    let data_clone = data.clone();

                    topology.set(Some(data));

                    #[cfg(target_arch = "wasm32")]
                    {
                        spawn(async move {
                            gloo_timers::future::TimeoutFuture::new(50).await;
                            init_cytoscape_graph(data_clone);
                        });
                    }
                }
                Err(e) => error_msg.set(Some(format!("Error: {}", e))),
            }
            loading.set(false);
        });
    });

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
                    div { class: "bg-blue-50 dark:bg-blue-900/30 border border-blue-200 dark:border-blue-800 text-blue-800 dark:text-blue-300 px-4 py-3 rounded mb-4 flex flex-col gap-3",
                        div { class: "flex flex-col lg:flex-row lg:items-center lg:justify-between gap-3",
                            span { "Showing {topo.nodes.len()} nodes and {topo.edges.len()} connections • Last updated: {format_timestamp(topo.timestamp.as_ref())}" }
                            div { class: "flex flex-wrap items-center gap-2",
                                button {
                                    class: format!("px-3 py-1 text-sm rounded border transition-colors {}",
                                        if topology_mode() == TopologySource::Deployments { "bg-blue-600 dark:bg-blue-500 text-white border-blue-700 dark:border-blue-400" } else { "bg-white dark:bg-gray-800 text-blue-700 dark:text-blue-300 border-blue-200 dark:border-blue-700" }),
                                    onclick: move |_| { if topology_mode() != TopologySource::Deployments { topology_mode.set(TopologySource::Deployments); } },
                                    "From Deployments"
                                }
                                button {
                                    class: format!("px-3 py-1 text-sm rounded border transition-colors {}",
                                        if topology_mode() == TopologySource::Zenoh { "bg-blue-600 dark:bg-blue-500 text-white border-blue-700 dark:border-blue-400" } else { "bg-white dark:bg-gray-800 text-blue-700 dark:text-blue-300 border-blue-200 dark:border-blue-700" }),
                                    onclick: move |_| { if topology_mode() != TopologySource::Zenoh { topology_mode.set(TopologySource::Zenoh); } },
                                    "From Zenoh"
                                }
                                button {
                                    class: "px-3 py-1 text-sm bg-blue-600 dark:bg-blue-500 text-white rounded hover:bg-blue-700 dark:hover:bg-blue-600 transition-colors",
                                    onclick: move |_| show_panel.set(!show_panel()),
                                    if show_panel() { "Hide Panel" } else { "Show Panel" }
                                }
                            }
                        }
                        div { class: "text-sm flex flex-wrap items-center gap-4",
                            span { class: "inline-flex items-center", span { class: "inline-block w-3 h-3 rounded bg-indigo-500 mr-1" } "Package" }
                            span { class: "inline-flex items-center", span { class: "inline-block w-3 h-3 rounded-full bg-cyan-500 mr-1" } "Class" }
                            span { class: "inline-flex items-center", span { class: "inline-block w-3 h-3 rounded bg-amber-500 mr-1" } "Function" }
                            span { class: "inline-flex items-center", span { class: "inline-block w-3 h-3 rotate-45 bg-pink-500 mr-1" } "Environment" }
                            span { class: "inline-flex items-center", span { class: "inline-block w-3 h-3 rotate-45 bg-purple-500 mr-1" } "Router" }
                            span { class: "inline-flex items-center", span { class: "inline-block w-3 h-3 rounded bg-blue-500 mr-1" } "Gateway" }
                            span { class: "inline-flex items-center", span { class: "inline-block w-3 h-3 rounded-full bg-emerald-500 mr-1" } "ODGM" }
                        }
                    }

                    div { class: "relative",
                        div { id: "cy-container", class: "w-full bg-white dark:bg-gray-800 rounded-lg shadow-lg border border-gray-200 dark:border-gray-700", style: "height: 600px;",
                            onmounted: move |_| {
                                #[cfg(target_arch = "wasm32")]
                                {
                                    let mut node_sig = selected_node_id;
                                    register_node_callback(move |node_id| {
                                        node_sig.set(node_id);
                                    });
                                }
                            },
                        }

                        if show_panel() {
                            div { class: "absolute top-0 right-0 h[600px] w-80 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 shadow-lg rounded-l-lg p-4 z-20", style: "overflow-y: auto;",
                                if let Some(node) = selected_node() {
                                    div {
                                        h3 { class: "text-lg font-bold mb-3 text-gray-900 dark:text-gray-100 border-b pb-2 dark:border-gray-700", "{node.id}" }
                                        div { class: "mb-4",
                                            div { class: "text-xs text-gray-500 dark:text-gray-400 uppercase mb-1", "Type" }
                                            div { class: "flex items-center gap-2",
                                                span { class: match node.node_type.as_str() {
                                                    "gateway" => "px-2 py-1 bg-blue-100 dark:bg-blue-900/30 text-blue-800 dark:text-blue-300 rounded text-sm",
                                                    "router" => "px-2 py-1 bg-purple-100 dark:bg-purple-900/30 text-purple-800 dark:text-purple-300 rounded text-sm",
                                                    "odgm" => "px-2 py-1 bg-emerald-100 dark:bg-emerald-900/30 text-emerald-800 dark:text-emerald-300 rounded text-sm",
                                                    "function" => "px-2 py-1 bg-amber-100 dark:bg-amber-900/30 text-amber-800 dark:text-amber-300 rounded text-sm",
                                                    "class" => "px-2 py-1 bg-cyan-100 dark:bg-cyan-900/30 text-cyan-800 dark:text-cyan-300 rounded text-sm",
                                                    "package" => "px-2 py-1 bg-indigo-100 dark:bg-indigo-900/30 text-indigo-800 dark:text-indigo-300 rounded text-sm",
                                                    "environment" => "px-2 py-1 bg-pink-100 dark:bg-pink-900/30 text-pink-800 dark:text-pink-300 rounded text-sm",
                                                    "partition" => "px-2 py-1 bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300 rounded text-sm",
                                                    "replica" => "px-2 py-1 bg-emerald-100 dark:bg-emerald-900/30 text-emerald-800 dark:text-emerald-300 rounded text-sm",
                                                    _ => "px-2 py-1 bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300 rounded text-sm",
                                                }, "{node.node_type}" }
                                            }
                                        }
                                        div { class: "mb-4",
                                            div { class: "text-xs text-gray-500 dark:text-gray-400 uppercase mb-1", "Status" }
                                            span { class: match node.status.as_str() {
                                                "healthy" => "px-2 py-1 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300 rounded text-sm font-semibold",
                                                "degraded" => "px-2 py-1 bg-yellow-100 dark:bg-yellow-900/30 text-yellow-800 dark:text-yellow-300 rounded text-sm font-semibold",
                                                "down" => "px-2 py-1 bg-red-100 dark:bg-red-900/30 text-red-800 dark:text-red-300 rounded text-sm font-semibold",
                                                _ => "px-2 py-1 bg-gray-100 dark:bg-gray-700 text-gray-800 dark:text-gray-300 rounded text-sm font-semibold",
                                            }, "{node.status}" }
                                        }
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
                                        if !node.deployed_classes.is_empty() {
                                            div { class: "mb-4",
                                                div { class: "text-xs text-gray-500 dark:text-gray-400 uppercase mb-2", "Deployed Classes" }
                                                div { class: "space-y-2",
                                                    for class_name in node.deployed_classes.iter() {
                                                        div { class: "bg-gray-50 dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded p-2",
                                                            div { class: "font-semibold text-sm text-gray-900 dark:text-gray-100 mb-1", "{class_name}" }
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
                                    div { class: "flex flex-col items-center justify-center h-full text-gray-400 dark:text-gray-500",
                                        div { class: "text-4xl mb-3", "👆" }
                                        div { class: "text-center text-sm", "Click on a node in the graph" br {} "to view details" }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "mt-4 text-sm text-gray-600 dark:text-gray-400 text-center",
                        "💡 Click and drag nodes • Scroll to zoom • Click nodes to view details"
                    }
                }
            }
        }
    }
}
