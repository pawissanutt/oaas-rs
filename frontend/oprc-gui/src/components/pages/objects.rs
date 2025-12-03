//! Objects page component - browse and inspect objects with invocation support

use crate::types::*;
use crate::{
    proxy_get_package, proxy_invoke, proxy_list_classes, proxy_list_objects,
    proxy_list_objects_all_partitions, proxy_object_delete, proxy_object_get,
    proxy_object_put,
};
use dioxus::prelude::*;
use oprc_grpc::{ObjMeta, ValData};
use oprc_models::FunctionBinding;

#[component]
pub fn Objects() -> Element {
    // Class & partition selection
    let mut classes = use_signal(|| Vec::<ClassRuntime>::new());
    let mut selected_class = use_signal(|| None::<ClassRuntime>);
    let mut partition_mode = use_signal(|| "all".to_string()); // "all" or specific number

    // Object list state
    let mut objects = use_signal(|| Vec::<ObjectListItem>::new());
    let mut prefix_filter = use_signal(|| "".to_string());
    let mut list_loading = use_signal(|| false);
    let mut list_error = use_signal(|| None::<String>);

    // Selected object detail
    let mut selected_object = use_signal(|| None::<ObjData>);
    let mut selected_object_item = use_signal(|| None::<ObjectListItem>); // Track the list item too
    let mut detail_loading = use_signal(|| false);
    let mut detail_error = use_signal(|| None::<String>);

    // Invoke state
    let mut selected_function = use_signal(|| "".to_string());
    let mut invoke_payload = use_signal(|| "{}".to_string());
    let mut invoke_response = use_signal(|| None::<String>);
    let mut invoke_loading = use_signal(|| false);
    // Function bindings from the package (for dropdown)
    let mut function_bindings = use_signal(|| Vec::<FunctionBinding>::new());

    // CRUD modal state
    let mut show_create_modal = use_signal(|| false);
    let mut show_edit_modal = use_signal(|| false);
    let mut crud_object_id = use_signal(|| "".to_string());
    let mut crud_partition_id = use_signal(|| 0u32);
    // Entries as Vec of (key, value_string, is_binary) tuples
    let mut crud_entries = use_signal(|| Vec::<(String, String, bool)>::new());
    let mut crud_loading = use_signal(|| false);
    let mut crud_error = use_signal(|| None::<String>);

    // Load classes on mount
    use_effect(move || {
        spawn(async move {
            match proxy_list_classes().await {
                Ok(cls_list) => {
                    // Try to restore last selected class from localStorage
                    let last_class = get_last_class();
                    if let Some(ref last_key) = last_class {
                        if let Some(cls) =
                            cls_list.iter().find(|c| &c.class_key == last_key)
                        {
                            selected_class.set(Some(cls.clone()));
                            // Also fetch package to get function bindings
                            let package_name = cls.package_name.clone();
                            let class_key = cls.class_key.clone();
                            spawn(async move {
                                if let Ok(pkg) =
                                    proxy_get_package(&package_name).await
                                {
                                    let class_key_suffix = class_key
                                        .strip_prefix(&format!(
                                            "{}.",
                                            package_name
                                        ))
                                        .unwrap_or(&class_key);
                                    if let Some(class_def) = pkg
                                        .classes
                                        .iter()
                                        .find(|c| c.key == class_key_suffix)
                                    {
                                        function_bindings.set(
                                            class_def.function_bindings.clone(),
                                        );
                                    } else if let Some(class_def) = pkg
                                        .classes
                                        .iter()
                                        .find(|c| c.key == class_key)
                                    {
                                        function_bindings.set(
                                            class_def.function_bindings.clone(),
                                        );
                                    }
                                }
                            });
                        }
                    }
                    classes.set(cls_list);
                }
                Err(e) => {
                    tracing::error!("Failed to load classes: {}", e);
                }
            }
        });
    });

    // Browse handler
    let browse_handler = move |_| {
        let cls = selected_class();
        if cls.is_none() {
            list_error.set(Some("Please select a class".to_string()));
            return;
        }
        let cls = cls.unwrap();

        spawn(async move {
            list_loading.set(true);
            list_error.set(None);
            objects.set(Vec::new());
            selected_object.set(None);

            let result = if partition_mode() == "all" {
                proxy_list_objects_all_partitions(
                    &cls.class_key,
                    cls.partition_count,
                    Some(&prefix_filter()),
                    Some(100),
                )
                .await
            } else {
                let pid: u32 = partition_mode().parse().unwrap_or(0);
                proxy_list_objects(
                    &cls.class_key,
                    pid,
                    Some(&prefix_filter()),
                    Some(100),
                    None,
                )
                .await
                .map(|r| r.objects)
            };

            match result {
                Ok(objs) => objects.set(objs),
                Err(e) => list_error.set(Some(format!("Error: {}", e))),
            }

            list_loading.set(false);
        });
    };

    // Object click handler
    let select_object = move |obj: ObjectListItem| {
        let cls = selected_class();
        if cls.is_none() {
            return;
        }
        let cls = cls.unwrap();
        let obj_item = obj.clone(); // Keep a copy of the list item

        spawn(async move {
            detail_loading.set(true);
            detail_error.set(None);

            let req = ObjectGetRequest {
                class_key: cls.class_key.clone(),
                partition_id: obj.partition_id.to_string(),
                object_id: obj.object_id.clone(),
            };

            match proxy_object_get(req).await {
                Ok(data) => {
                    selected_object.set(Some(data));
                    selected_object_item.set(Some(obj_item));
                }
                Err(e) => detail_error.set(Some(format!("Error: {}", e))),
            }

            detail_loading.set(false);
        });
    };

    // Invoke handler
    let invoke_handler = move |_| {
        let cls = selected_class();
        let obj_item = selected_object_item();
        if cls.is_none() || obj_item.is_none() {
            return;
        }
        let cls = cls.unwrap();
        let obj_item = obj_item.unwrap();

        let func_key = selected_function();
        if func_key.is_empty() {
            invoke_response.set(Some("Please select a function".to_string()));
            return;
        }

        // Check if the selected function is stateless
        let is_stateless = function_bindings()
            .iter()
            .find(|f| f.name == func_key)
            .is_some_and(|f| f.stateless);

        // For stateless functions, skip object_id (use stateless invocation path)
        // For stateful functions, include object_id (use object-bound invocation path)
        let object_id = if is_stateless {
            None
        } else {
            Some(obj_item.object_id.clone())
        };
        // Use the partition_id from the selected object list item
        let partition_id = obj_item.partition_id.to_string();

        spawn(async move {
            invoke_loading.set(true);
            invoke_response.set(None);

            let payload: serde_json::Value =
                serde_json::from_str(&invoke_payload())
                    .unwrap_or(serde_json::json!({}));

            let req = InvokeRequest {
                class_key: cls.class_key.clone(),
                partition_id,
                function_key: func_key,
                payload,
                object_id,
            };

            match proxy_invoke(req).await {
                Ok(resp) => {
                    let result = if let Some(ref payload) = resp.payload {
                        String::from_utf8_lossy(payload).to_string()
                    } else {
                        "Empty response".to_string()
                    };
                    invoke_response.set(Some(format!(
                        "✓ Status: {}\n{}",
                        resp.status, result
                    )));
                }
                Err(e) => {
                    invoke_response.set(Some(format!("✗ Error: {}", e)));
                }
            }

            invoke_loading.set(false);
        });
    };

    // Delete object handler
    let delete_handler = move |_: dioxus::events::MouseEvent| {
        let cls = selected_class();
        let obj_item = selected_object_item();
        if cls.is_none() || obj_item.is_none() {
            return;
        }
        let cls = cls.unwrap();
        let obj_item = obj_item.unwrap();

        spawn(async move {
            crud_loading.set(true);
            crud_error.set(None);

            match proxy_object_delete(
                &cls.class_key,
                obj_item.partition_id,
                &obj_item.object_id,
            )
            .await
            {
                Ok(_) => {
                    // Remove from list and clear selection
                    objects.write().retain(|o| {
                        !(o.object_id == obj_item.object_id
                            && o.partition_id == obj_item.partition_id)
                    });
                    selected_object.set(None);
                    selected_object_item.set(None);
                }
                Err(e) => crud_error.set(Some(format!("Delete failed: {}", e))),
            }

            crud_loading.set(false);
        });
    };

    // Open create modal
    let open_create_modal = move |_: dioxus::events::MouseEvent| {
        let pid = if partition_mode() == "all" {
            0u32
        } else {
            partition_mode().parse().unwrap_or(0)
        };
        crud_object_id.set("".to_string());
        crud_partition_id.set(pid);
        crud_entries.set(Vec::new());
        crud_error.set(None);
        show_create_modal.set(true);
    };

    // Open edit modal
    let open_edit_modal = move |_: dioxus::events::MouseEvent| {
        let obj_item = selected_object_item();
        let obj = selected_object();
        if obj_item.is_none() || obj.is_none() {
            return;
        }
        let obj_item = obj_item.unwrap();
        let obj = obj.unwrap();

        crud_object_id.set(obj_item.object_id.clone());
        crud_partition_id.set(obj_item.partition_id);

        // Convert entries to Vec of (key, value, is_binary)
        let mut entries_vec: Vec<(String, String, bool)> = Vec::new();
        for (key, val) in obj.entries.iter() {
            let (value_str, is_binary) = match std::str::from_utf8(&val.data) {
                Ok(s) => (s.to_string(), false),
                Err(_) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD
                        .encode(&val.data);
                    (b64, true)
                }
            };
            entries_vec.push((key.clone(), value_str, is_binary));
        }
        // Sort by key for consistent ordering
        entries_vec.sort_by(|a, b| a.0.cmp(&b.0));
        crud_entries.set(entries_vec);
        crud_error.set(None);
        show_edit_modal.set(true);
    };

    // Save object (create or update)
    let mut save_object_handler = {
        move |is_create: bool| {
            let cls = selected_class();
            if cls.is_none() {
                crud_error.set(Some("No class selected".to_string()));
                return;
            }
            let cls = cls.unwrap();
            let object_id = crud_object_id();
            let partition_id = crud_partition_id();
            let entries_list = crud_entries();

            if object_id.is_empty() {
                crud_error.set(Some("Object ID is required".to_string()));
                return;
            }

            // Convert entries list to HashMap
            let mut entries = std::collections::HashMap::new();
            for (key, value, is_binary) in entries_list.iter() {
                if key.is_empty() {
                    continue; // Skip empty keys
                }
                let data = if *is_binary {
                    // Decode base64
                    use base64::Engine;
                    match base64::engine::general_purpose::STANDARD
                        .decode(value)
                    {
                        Ok(bytes) => bytes,
                        Err(_) => {
                            crud_error.set(Some(format!(
                                "Invalid base64 for key '{}'",
                                key
                            )));
                            return;
                        }
                    }
                } else {
                    value.as_bytes().to_vec()
                };
                entries.insert(
                    key.clone(),
                    ValData {
                        data,
                        r#type: 0, // VAL_TYPE_BYTE
                    },
                );
            }

            let entry_count = entries.len() as u64;
            let obj_data = ObjData {
                metadata: Some(ObjMeta {
                    cls_id: cls.class_key.clone(),
                    partition_id,
                    object_id: Some(object_id.clone()),
                }),
                entries,
                event: None,
            };

            spawn(async move {
                crud_loading.set(true);
                crud_error.set(None);

                let req = ObjectPutRequest {
                    class_key: cls.class_key.clone(),
                    partition_id: partition_id.to_string(),
                    object_id: object_id.clone(),
                    data: obj_data,
                };

                match proxy_object_put(req).await {
                    Ok(_) => {
                        show_create_modal.set(false);
                        show_edit_modal.set(false);

                        // If creating, add to list; if editing, reload detail
                        if is_create {
                            objects.write().push(ObjectListItem {
                                object_id: object_id.clone(),
                                partition_id,
                                version: 1,
                                entry_count,
                            });
                        } else {
                            // Reload the object detail
                            let req = ObjectGetRequest {
                                class_key: cls.class_key.clone(),
                                partition_id: partition_id.to_string(),
                                object_id: object_id.clone(),
                            };
                            if let Ok(data) = proxy_object_get(req).await {
                                selected_object.set(Some(data));
                            }
                        }
                    }
                    Err(e) => {
                        crud_error.set(Some(format!("Save failed: {}", e)))
                    }
                }

                crud_loading.set(false);
            });
        }
    };

    rsx! {
        div { class: "container mx-auto p-6",
            h1 { class: "text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100",
                "Objects Browser"
            }

            // Top bar: Class & Partition selector
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-4 mb-6",
                div { class: "flex flex-wrap gap-4 items-end",
                    // Class dropdown
                    div { class: "flex-1 min-w-48",
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Class"
                        }
                        select {
                            class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            onchange: move |e| {
                                let key = e.value();
                                if key.is_empty() {
                                    selected_class.set(None);
                                    function_bindings.set(Vec::new());
                                    selected_function.set("".to_string());
                                } else {
                                    if let Some(cls) = classes().iter().find(|c| c.class_key == key) {
                                        save_last_class(&cls.class_key);
                                        selected_class.set(Some(cls.clone()));
                                        // Reset function selection and fetch package to get function bindings
                                        selected_function.set("".to_string());
                                        // Fetch package to get function bindings for the class
                                        let package_name = cls.package_name.clone();
                                        let class_key = cls.class_key.clone();
                                        spawn(async move {
                                            match proxy_get_package(&package_name).await {
                                                Ok(pkg) => {
                                                    // Find the class in the package and get its function bindings
                                                    // class_key format is "{package_name}.{class_key_in_package}"
                                                    let class_key_suffix = class_key.strip_prefix(&format!("{}.", package_name))
                                                        .unwrap_or(&class_key);
                                                    if let Some(class_def) = pkg.classes.iter().find(|c| c.key == class_key_suffix) {
                                                        function_bindings.set(class_def.function_bindings.clone());
                                                    } else {
                                                        // Fallback: try exact match
                                                        if let Some(class_def) = pkg.classes.iter().find(|c| c.key == class_key) {
                                                            function_bindings.set(class_def.function_bindings.clone());
                                                        } else {
                                                            function_bindings.set(Vec::new());
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!("Failed to load package {}: {}", package_name, e);
                                                    function_bindings.set(Vec::new());
                                                }
                                            }
                                        });
                                    }
                                }
                            },
                            option { value: "", "Select a class..." }
                            for cls in classes() {
                                option {
                                    value: "{cls.class_key}",
                                    selected: selected_class().as_ref().is_some_and(|s| s.class_key == cls.class_key),
                                    "{cls.class_key}"
                                }
                            }
                        }
                    }

                    // Partition selector
                    div { class: "w-32",
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Partition"
                        }
                        select {
                            class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            onchange: move |e| partition_mode.set(e.value()),
                            option { value: "all", "All" }
                            if let Some(cls) = selected_class() {
                                for i in 0..cls.partition_count {
                                    option { value: "{i}", "{i}" }
                                }
                            }
                        }
                    }

                    // Show partition count (read-only, from class)
                    if let Some(cls) = selected_class() {
                        div { class: "w-20",
                            label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                                "Parts"
                            }
                            div {
                                class: "px-3 py-2 border border-gray-200 dark:border-gray-600 rounded-md bg-gray-100 dark:bg-gray-600 text-gray-700 dark:text-gray-300 text-center",
                                "{cls.partition_count}"
                            }
                        }
                    }

                    // Prefix filter
                    div { class: "flex-1 min-w-32",
                        label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                            "Prefix Filter"
                        }
                        input {
                            class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            r#type: "text",
                            placeholder: "e.g., user-",
                            value: "{prefix_filter}",
                            oninput: move |e| prefix_filter.set(e.value())
                        }
                    }

                    // Browse button
                    div {
                        label { class: "block text-sm font-medium text-transparent mb-1", "." }
                        button {
                            class: "px-6 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:bg-gray-400 transition-colors",
                            disabled: list_loading() || selected_class().is_none(),
                            onclick: browse_handler,
                            if list_loading() { "Loading..." } else { "Browse" }
                        }
                    }

                    // Create button
                    div {
                        label { class: "block text-sm font-medium text-transparent mb-1", "." }
                        button {
                            class: "px-4 py-2 bg-green-600 text-white rounded-md hover:bg-green-700 disabled:bg-gray-400 transition-colors",
                            disabled: selected_class().is_none(),
                            onclick: open_create_modal,
                            "+ New"
                        }
                    }
                }
            }

            // Error display
            if let Some(err) = list_error() {
                div { class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded mb-6",
                    "{err}"
                }
            }

            // CRUD error display
            if let Some(err) = crud_error() {
                div { class: "bg-red-50 dark:bg-red-900/30 border border-red-200 dark:border-red-800 text-red-700 dark:text-red-300 px-4 py-3 rounded mb-6",
                    "{err}"
                }
            }

            // Main content: two-panel layout
            div { class: "flex gap-6",
                // Left panel: Object list
                div { class: "w-1/2 bg-white dark:bg-gray-800 rounded-lg shadow",
                    div { class: "p-4 border-b border-gray-200 dark:border-gray-700",
                        h2 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100",
                            "Objects"
                        }
                    }

                    if objects().is_empty() && !list_loading() {
                        div { class: "p-8 text-center text-gray-500 dark:text-gray-400",
                            if selected_class().is_none() {
                                "Select a class and click Browse"
                            } else {
                                "No objects found"
                            }
                        }
                    } else {
                        // Object table
                        div { class: "overflow-auto max-h-96",
                            table { class: "w-full text-sm",
                                thead { class: "bg-gray-50 dark:bg-gray-900 sticky top-0",
                                    tr {
                                        th { class: "px-4 py-2 text-left text-gray-600 dark:text-gray-400", "Object ID" }
                                        if partition_mode() == "all" {
                                            th { class: "px-2 py-2 text-center text-gray-600 dark:text-gray-400", "P#" }
                                        }
                                        th { class: "px-2 py-2 text-center text-gray-600 dark:text-gray-400", "Ver" }
                                        th { class: "px-2 py-2 text-center text-gray-600 dark:text-gray-400", "Entries" }
                                    }
                                }
                                tbody {
                                    for obj in objects() {
                                        {
                                            let obj_clone = obj.clone();
                                            rsx! {
                                                tr {
                                                    class: "border-t border-gray-100 dark:border-gray-700 hover:bg-blue-50 dark:hover:bg-blue-900/20 cursor-pointer",
                                                    onclick: move |_| select_object(obj_clone.clone()),
                                                    td { class: "px-4 py-2 font-mono text-gray-900 dark:text-gray-100",
                                                        "{obj.object_id}"
                                                    }
                                                    if partition_mode() == "all" {
                                                        td { class: "px-2 py-2 text-center text-gray-600 dark:text-gray-400",
                                                            "{obj.partition_id}"
                                                        }
                                                    }
                                                    td { class: "px-2 py-2 text-center text-gray-600 dark:text-gray-400",
                                                        "v{obj.version}"
                                                    }
                                                    td { class: "px-2 py-2 text-center text-gray-600 dark:text-gray-400",
                                                        "{obj.entry_count}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "p-3 border-t border-gray-200 dark:border-gray-700 text-sm text-gray-500 dark:text-gray-400",
                            "--- {objects().len()} object(s) ---"
                        }
                    }
                }

                // Right panel: Object detail + Invoke
                div { class: "w-1/2 space-y-4",
                    // Object detail
                    div { class: "bg-white dark:bg-gray-800 rounded-lg shadow",
                        div { class: "p-4 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center",
                            h2 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100",
                                "Object Detail"
                            }
                            if selected_object().is_some() {
                                div { class: "flex gap-2",
                                    button {
                                        class: "px-3 py-1 text-sm bg-yellow-500 text-white rounded hover:bg-yellow-600 disabled:bg-gray-400",
                                        disabled: crud_loading(),
                                        onclick: open_edit_modal,
                                        "Edit"
                                    }
                                    button {
                                        class: "px-3 py-1 text-sm bg-red-600 text-white rounded hover:bg-red-700 disabled:bg-gray-400",
                                        disabled: crud_loading(),
                                        onclick: delete_handler,
                                        if crud_loading() { "..." } else { "Delete" }
                                    }
                                }
                            }
                        }

                        if detail_loading() {
                            div { class: "p-8 text-center",
                                div { class: "inline-block animate-spin rounded-full h-6 w-6 border-b-2 border-blue-600" }
                            }
                        } else if let Some(err) = detail_error() {
                            div { class: "p-4 text-red-600 dark:text-red-400", "{err}" }
                        } else if let Some(obj) = selected_object() {
                            div { class: "p-4",
                                // Metadata
                                if let Some(meta) = &obj.metadata {
                                    dl { class: "grid grid-cols-2 gap-2 text-sm mb-4",
                                        dt { class: "text-gray-600 dark:text-gray-400", "Class:" }
                                        dd { class: "font-mono text-gray-900 dark:text-gray-100", "{meta.cls_id}" }
                                        dt { class: "text-gray-600 dark:text-gray-400", "Partition:" }
                                        dd { class: "font-mono text-gray-900 dark:text-gray-100", "{meta.partition_id}" }
                                        dt { class: "text-gray-600 dark:text-gray-400", "Object ID:" }
                                        dd { class: "font-mono text-gray-900 dark:text-gray-100",
                                            {meta.object_id.clone().unwrap_or_else(|| "-".to_string())}
                                        }
                                    }
                                }

                                // Entries
                                if !obj.entries.is_empty() {
                                    h3 { class: "text-sm font-semibold text-gray-500 dark:text-gray-400 uppercase mb-2",
                                        "ENTRIES"
                                    }
                                    div { class: "space-y-2 max-h-64 overflow-auto",
                                        for (key, val) in obj.entries.iter() {
                                            {
                                                // Check if data is valid UTF-8
                                                let (value_str, is_binary) = match std::str::from_utf8(&val.data) {
                                                    Ok(s) => (s.to_string(), false),
                                                    Err(_) => {
                                                        // Binary data - encode as base64
                                                        use base64::Engine;
                                                        let b64 = base64::engine::general_purpose::STANDARD.encode(&val.data);
                                                        (b64, true)
                                                    }
                                                };
                                                let is_long = value_str.len() > 100;
                                                let display_value = if is_long {
                                                    format!("{}...", &value_str.chars().take(100).collect::<String>())
                                                } else {
                                                    value_str.clone()
                                                };
                                                let data_len = val.data.len();
                                                rsx! {
                                                    div { class: "bg-gray-50 dark:bg-gray-900 p-2 rounded text-sm group relative",
                                                        div { class: "flex items-start justify-between gap-2",
                                                            div { class: "flex-1 min-w-0",
                                                                span { class: "font-mono text-green-600 dark:text-green-400 font-semibold", "{key}" }
                                                                span { class: "text-gray-500 mx-1", "=" }
                                                                if is_binary {
                                                                    span { class: "text-xs text-orange-500 dark:text-orange-400 mr-1", "[binary]" }
                                                                }
                                                                span {
                                                                    class: "font-mono text-gray-700 dark:text-gray-300 break-all select-all",
                                                                    title: "{value_str}",
                                                                    "{display_value}"
                                                                }
                                                                if is_long || is_binary {
                                                                    span { class: "text-xs text-gray-400 ml-1", "({data_len} bytes)" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    p { class: "text-gray-500 dark:text-gray-400 text-sm",
                                        "No entries"
                                    }
                                }
                            }
                        } else {
                            div { class: "p-8 text-center text-gray-500 dark:text-gray-400",
                                "Click an object to view details"
                            }
                        }
                    }

                    // Invoke section
                    if selected_object().is_some() && selected_class().is_some() {
                        div { class: "bg-white dark:bg-gray-800 rounded-lg shadow",
                            div { class: "p-4 border-b border-gray-200 dark:border-gray-700",
                                h2 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100",
                                    "Invoke Function"
                                }
                            }

                            div { class: "p-4 space-y-3",
                                // Function name dropdown
                                div {
                                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                                        "Function Name"
                                    }
                                    select {
                                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                                        onchange: move |e| selected_function.set(e.value()),
                                        option { value: "", "Select a function..." }
                                        // Stateful functions optgroup
                                        {
                                            let stateful: Vec<_> = function_bindings().iter().filter(|f| !f.stateless).cloned().collect();
                                            rsx! {
                                                if !stateful.is_empty() {
                                                    optgroup { label: "Stateful Functions",
                                                        for func in stateful {
                                                            option {
                                                                value: "{func.name}",
                                                                selected: selected_function() == func.name,
                                                                "{func.name}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // Stateless functions optgroup
                                        {
                                            let stateless: Vec<_> = function_bindings().iter().filter(|f| f.stateless).cloned().collect();
                                            rsx! {
                                                if !stateless.is_empty() {
                                                    optgroup { label: "Stateless Functions",
                                                        for func in stateless {
                                                            option {
                                                                value: "{func.name}",
                                                                selected: selected_function() == func.name,
                                                                "{func.name}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    // Show info about the selected function
                                    if !selected_function().is_empty() {
                                        if let Some(func) = function_bindings().iter().find(|f| f.name == selected_function()) {
                                            div { class: "mt-1 text-xs text-gray-500 dark:text-gray-400",
                                                if func.stateless {
                                                    span { class: "inline-flex items-center px-2 py-0.5 rounded bg-blue-100 dark:bg-blue-900 text-blue-800 dark:text-blue-200",
                                                        "Stateless"
                                                    }
                                                } else {
                                                    span { class: "inline-flex items-center px-2 py-0.5 rounded bg-green-100 dark:bg-green-900 text-green-800 dark:text-green-200",
                                                        "Stateful"
                                                    }
                                                }
                                                span { class: "ml-2", "→ {func.function_key}" }
                                            }
                                        }
                                    }
                                }

                                // Payload
                                div {
                                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                                        "Payload (JSON)"
                                    }
                                    textarea {
                                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 font-mono text-sm",
                                        rows: "3",
                                        value: "{invoke_payload}",
                                        oninput: move |e| invoke_payload.set(e.value())
                                    }
                                }

                                // Invoke button
                                button {
                                    class: "w-full px-4 py-2 bg-green-600 text-white rounded-md hover:bg-green-700 disabled:bg-gray-400 transition-colors",
                                    disabled: invoke_loading() || selected_function().is_empty(),
                                    onclick: invoke_handler,
                                    if invoke_loading() { "Invoking..." } else { "Invoke" }
                                }

                                // Response
                                if let Some(resp) = invoke_response() {
                                    pre { class: "mt-2 p-3 bg-gray-50 dark:bg-gray-900 rounded text-sm font-mono whitespace-pre-wrap overflow-auto max-h-32",
                                        "{resp}"
                                    }
                                }
                            }
                        }
                    }

                    // Traces placeholder (future)
                    div { class: "bg-white dark:bg-gray-800 rounded-lg shadow opacity-50",
                        div { class: "p-4",
                            h3 { class: "text-sm font-medium text-gray-500 dark:text-gray-400",
                                "🔍 Traces (Coming Soon)"
                            }
                        }
                    }
                }
            }

            // Create/Edit Modal
            if show_create_modal() || show_edit_modal() {
                div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    div { class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl w-full max-w-2xl mx-4 max-h-[90vh] flex flex-col",
                        // Modal header
                        div { class: "p-4 border-b border-gray-200 dark:border-gray-700 flex justify-between items-center flex-shrink-0",
                            h3 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100",
                                if show_create_modal() { "Create Object" } else { "Edit Object" }
                            }
                            button {
                                class: "text-gray-400 hover:text-gray-600 dark:hover:text-gray-200",
                                onclick: move |_| {
                                    show_create_modal.set(false);
                                    show_edit_modal.set(false);
                                },
                                "✕"
                            }
                        }

                        // Modal body - scrollable
                        div { class: "p-4 space-y-4 overflow-y-auto flex-1",
                            // Object ID
                            div {
                                label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                                    "Object ID"
                                }
                                input {
                                    class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                                    r#type: "text",
                                    placeholder: "my-object-id",
                                    value: "{crud_object_id}",
                                    disabled: show_edit_modal(),
                                    oninput: move |e| crud_object_id.set(e.value())
                                }
                            }

                            // Partition ID (only for create)
                            if show_create_modal() {
                                div {
                                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1",
                                        "Partition ID"
                                    }
                                    input {
                                        class: "w-full px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                                        r#type: "number",
                                        min: "0",
                                        value: "{crud_partition_id}",
                                        oninput: move |e| {
                                            if let Ok(n) = e.value().parse::<u32>() {
                                                crud_partition_id.set(n);
                                            }
                                        }
                                    }
                                }
                            }

                            // Entries as key-value pairs
                            div {
                                div { class: "flex justify-between items-center mb-2",
                                    label { class: "block text-sm font-medium text-gray-700 dark:text-gray-300",
                                        "Entries"
                                    }
                                    button {
                                        class: "px-2 py-1 text-xs bg-green-600 text-white rounded hover:bg-green-700",
                                        onclick: move |_| {
                                            crud_entries.write().push(("".to_string(), "".to_string(), false));
                                        },
                                        "+ Add Entry"
                                    }
                                }

                                // Entry list
                                div { class: "space-y-2 max-h-64 overflow-y-auto",
                                    for (idx, (key, value, is_binary)) in crud_entries().iter().enumerate() {
                                        {
                                            let key_clone = key.clone();
                                            let value_clone = value.clone();
                                            let is_binary_clone = *is_binary;
                                            rsx! {
                                                div { class: "flex gap-2 items-start bg-gray-50 dark:bg-gray-900 p-2 rounded",
                                                    // Key input
                                                    input {
                                                        class: "w-1/3 px-2 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 font-mono",
                                                        r#type: "text",
                                                        placeholder: "key",
                                                        value: "{key_clone}",
                                                        oninput: move |e| {
                                                            let mut entries = crud_entries();
                                                            if let Some(entry) = entries.get_mut(idx) {
                                                                entry.0 = e.value();
                                                            }
                                                            crud_entries.set(entries);
                                                        }
                                                    }

                                                    // Value input or binary indicator
                                                    div { class: "flex-1 flex gap-1",
                                                        if is_binary_clone {
                                                            div { class: "flex-1",
                                                                div { class: "px-2 py-1 text-xs text-orange-600 dark:text-orange-400 bg-orange-50 dark:bg-orange-900/20 rounded",
                                                                    "Binary data ({value_clone.len()} bytes base64)"
                                                                }
                                                            }
                                                        } else {
                                                            input {
                                                                class: "flex-1 px-2 py-1 text-sm border border-gray-300 dark:border-gray-600 rounded bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 font-mono",
                                                                r#type: "text",
                                                                placeholder: "value",
                                                                value: "{value_clone}",
                                                                oninput: move |e| {
                                                                    let mut entries = crud_entries();
                                                                    if let Some(entry) = entries.get_mut(idx) {
                                                                        entry.1 = e.value();
                                                                    }
                                                                    crud_entries.set(entries);
                                                                }
                                                            }
                                                        }

                                                        // File upload button
                                                        label { class: "px-2 py-1 text-xs bg-blue-500 text-white rounded hover:bg-blue-600 cursor-pointer flex items-center",
                                                            title: "Upload file",
                                                            "📁"
                                                            input {
                                                                class: "hidden",
                                                                r#type: "file",
                                                                onchange: move |e| {
                                                                    let files = e.files();
                                                                    if let Some(file_data) = files.first() {
                                                                        let file_data = file_data.clone();
                                                                        spawn(async move {
                                                                            match file_data.read_bytes().await {
                                                                                Ok(contents) => {
                                                                                    use base64::Engine;
                                                                                    let b64 = base64::engine::general_purpose::STANDARD.encode(&contents);
                                                                                    let mut entries = crud_entries();
                                                                                    if let Some(entry) = entries.get_mut(idx) {
                                                                                        entry.1 = b64;
                                                                                        entry.2 = true; // Mark as binary
                                                                                    }
                                                                                    crud_entries.set(entries);
                                                                                }
                                                                                Err(e) => {
                                                                                    tracing::error!("Failed to read file: {:?}", e);
                                                                                }
                                                                            }
                                                                        });
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }

                                                    // Delete button
                                                    button {
                                                        class: "px-2 py-1 text-xs text-red-600 hover:bg-red-100 dark:hover:bg-red-900/20 rounded",
                                                        onclick: move |_| {
                                                            let mut entries = crud_entries();
                                                            entries.remove(idx);
                                                            crud_entries.set(entries);
                                                        },
                                                        "✕"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                if crud_entries().is_empty() {
                                    p { class: "text-xs text-gray-500 dark:text-gray-400 mt-1 italic",
                                        "No entries. Click '+ Add Entry' to add key-value pairs."
                                    }
                                }
                            }

                            // Error in modal
                            if let Some(err) = crud_error() {
                                div { class: "text-red-600 dark:text-red-400 text-sm", "{err}" }
                            }
                        }

                        // Modal footer
                        div { class: "p-4 border-t border-gray-200 dark:border-gray-700 flex justify-end gap-2 flex-shrink-0",
                            button {
                                class: "px-4 py-2 text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-md",
                                onclick: move |_| {
                                    show_create_modal.set(false);
                                    show_edit_modal.set(false);
                                },
                                "Cancel"
                            }
                            button {
                                class: "px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:bg-gray-400",
                                disabled: crud_loading(),
                                onclick: move |_| save_object_handler(show_create_modal()),
                                if crud_loading() { "Saving..." } else { "Save" }
                            }
                        }
                    }
                }
            }
        }
    }
}

// LocalStorage helpers for remembering last class
fn get_last_class() -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        storage.get_item("oprc_last_class").ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

fn save_last_class(key: &str) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                let _ = storage.set_item("oprc_last_class", key);
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = key;
    }
}
