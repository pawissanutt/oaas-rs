//! YAML Editor component with validation, line numbers, and syntax highlighting

use dioxus::prelude::*;
use oprc_models::OPackage;

/// Validation result for YAML content
#[derive(Clone, PartialEq)]
pub enum ValidationResult {
    /// Not validated yet
    Pending,
    /// Valid YAML that parses correctly
    Valid,
    /// YAML syntax error
    YamlError(String),
    /// YAML is valid but doesn't match expected schema
    SchemaError(String),
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        matches!(self, ValidationResult::Valid)
    }
}

/// Apply YAML syntax highlighting to content
fn highlight_yaml(content: &str) -> String {
    let mut result = String::new();
    
    for line in content.lines() {
        let highlighted = highlight_yaml_line(line);
        result.push_str(&highlighted);
        result.push('\n');
    }
    
    // Remove trailing newline if original didn't have one
    if !content.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }
    
    result
}

/// Highlight a single YAML line
fn highlight_yaml_line(line: &str) -> String {
    // Handle empty lines
    if line.trim().is_empty() {
        return html_escape(line);
    }
    
    // Handle comments
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        let indent = &line[..line.len() - trimmed.len()];
        return format!(
            r#"{}<span class="yaml-comment">{}</span>"#,
            html_escape(indent),
            html_escape(trimmed)
        );
    }
    
    // Handle key-value pairs
    if let Some(colon_pos) = find_yaml_colon(line) {
        let (key_part, rest) = line.split_at(colon_pos);
        let (colon, value_part) = rest.split_at(1);
        
        // Check if key starts with a dash (list item)
        let trimmed_key = key_part.trim_start();
        if trimmed_key.starts_with('-') {
            let indent = &key_part[..key_part.len() - trimmed_key.len()];
            let after_dash = trimmed_key.strip_prefix('-').unwrap_or("");
            let after_dash_trimmed = after_dash.trim_start();
            let dash_space = &after_dash[..after_dash.len() - after_dash_trimmed.len()];
            
            return format!(
                r#"{}<span class="yaml-dash">-</span>{}<span class="yaml-key">{}</span><span class="yaml-colon">{}</span>{}"#,
                html_escape(indent),
                html_escape(dash_space),
                html_escape(after_dash_trimmed),
                html_escape(colon),
                highlight_yaml_value(value_part)
            );
        }
        
        let indent = &key_part[..key_part.len() - trimmed_key.len()];
        return format!(
            r#"{}<span class="yaml-key">{}</span><span class="yaml-colon">{}</span>{}"#,
            html_escape(indent),
            html_escape(trimmed_key),
            html_escape(colon),
            highlight_yaml_value(value_part)
        );
    }
    
    // Handle list items without key
    let trimmed = line.trim_start();
    if trimmed.starts_with('-') {
        let indent = &line[..line.len() - trimmed.len()];
        let after_dash = trimmed.strip_prefix('-').unwrap_or("");
        return format!(
            r#"{}<span class="yaml-dash">-</span>{}"#,
            html_escape(indent),
            highlight_yaml_value(after_dash)
        );
    }
    
    // Default: just escape
    html_escape(line)
}

/// Find the position of a YAML key-value colon (not inside quotes)
fn find_yaml_colon(line: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut quote_char = ' ';
    
    for (i, c) in line.char_indices() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            c if c == quote_char && in_quotes => {
                in_quotes = false;
            }
            ':' if !in_quotes => {
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

/// Highlight a YAML value
fn highlight_yaml_value(value: &str) -> String {
    let trimmed = value.trim();
    
    if trimmed.is_empty() {
        return html_escape(value);
    }
    
    // Leading whitespace
    let leading = &value[..value.len() - value.trim_start().len()];
    let content = value.trim_start();
    
    // Check for inline comment
    if let Some(comment_pos) = find_comment_start(content) {
        let (val, comment) = content.split_at(comment_pos);
        return format!(
            r#"{}{}<span class="yaml-comment">{}</span>"#,
            html_escape(leading),
            highlight_value_content(val.trim_end()),
            html_escape(comment)
        );
    }
    
    format!("{}{}", html_escape(leading), highlight_value_content(content))
}

/// Find start of inline comment (# not in quotes)
fn find_comment_start(s: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut quote_char = ' ';
    
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' if !in_quotes => {
                in_quotes = true;
                quote_char = c;
            }
            c if c == quote_char && in_quotes => {
                in_quotes = false;
            }
            '#' if !in_quotes => {
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

/// Highlight the actual value content
fn highlight_value_content(value: &str) -> String {
    let trimmed = value.trim();
    
    // Strings in quotes
    if (trimmed.starts_with('"') && trimmed.ends_with('"')) ||
       (trimmed.starts_with('\'') && trimmed.ends_with('\'')) {
        return format!(r#"<span class="yaml-string">{}</span>"#, html_escape(trimmed));
    }
    
    // Booleans
    let lower = trimmed.to_lowercase();
    if lower == "true" || lower == "false" || lower == "yes" || lower == "no" {
        return format!(r#"<span class="yaml-bool">{}</span>"#, html_escape(trimmed));
    }
    
    // Null
    if lower == "null" || lower == "~" || trimmed.is_empty() {
        return format!(r#"<span class="yaml-null">{}</span>"#, html_escape(trimmed));
    }
    
    // Numbers
    if trimmed.parse::<f64>().is_ok() || trimmed.parse::<i64>().is_ok() {
        return format!(r#"<span class="yaml-number">{}</span>"#, html_escape(trimmed));
    }
    
    // Default string (unquoted)
    format!(r#"<span class="yaml-string">{}</span>"#, html_escape(trimmed))
}

/// HTML escape special characters
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Validate YAML content as an OPackage
pub fn validate_package_yaml(content: &str) -> ValidationResult {
    if content.trim().is_empty() {
        return ValidationResult::YamlError("YAML content is empty".to_string());
    }

    // First check if it's valid YAML
    match serde_yaml::from_str::<serde_yaml::Value>(content) {
        Ok(_) => {}
        Err(e) => {
            return ValidationResult::YamlError(format!("YAML syntax error: {}", e));
        }
    }

    // Then try to parse as OPackage
    match serde_yaml::from_str::<OPackage>(content) {
        Ok(pkg) => {
            // Additional validation
            if pkg.name.is_empty() {
                return ValidationResult::SchemaError("Package name is required".to_string());
            }
            if pkg.classes.is_empty() && pkg.functions.is_empty() {
                return ValidationResult::SchemaError(
                    "Package must have at least one class or function".to_string(),
                );
            }
            // Check for duplicate class keys
            let mut class_keys = std::collections::HashSet::new();
            for class in &pkg.classes {
                if !class_keys.insert(&class.key) {
                    return ValidationResult::SchemaError(format!(
                        "Duplicate class key: {}",
                        class.key
                    ));
                }
            }
            // Check for duplicate function keys
            let mut fn_keys = std::collections::HashSet::new();
            for func in &pkg.functions {
                if !fn_keys.insert(&func.key) {
                    return ValidationResult::SchemaError(format!(
                        "Duplicate function key: {}",
                        func.key
                    ));
                }
            }
            ValidationResult::Valid
        }
        Err(e) => {
            // Try to provide a more helpful error message
            let err_str = e.to_string();
            if err_str.contains("missing field") {
                ValidationResult::SchemaError(format!("Missing required field: {}", err_str))
            } else if err_str.contains("unknown field") {
                ValidationResult::SchemaError(format!("Unknown field: {}", err_str))
            } else {
                ValidationResult::SchemaError(format!("Schema validation failed: {}", err_str))
            }
        }
    }
}

/// Generate line numbers for the content
fn generate_line_numbers(content: &str) -> String {
    let line_count = content.lines().count().max(1);
    // Add extra line if content ends with newline
    let extra = if content.ends_with('\n') { 1 } else { 0 };
    (1..=line_count + extra)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// YAML Editor component with syntax validation, line numbers, and highlighting
#[component]
pub fn YamlEditor(
    /// The YAML content signal
    value: Signal<String>,
    /// Placeholder text
    #[props(default = "Enter YAML content...")]
    placeholder: &'static str,
    /// Height class (default h-96)
    #[props(default = "h-96")]
    height: &'static str,
    /// Whether to validate as OPackage schema
    #[props(default = true)]
    validate_schema: bool,
    /// Callback when validation state changes
    #[props(default)]
    on_validation_change: Option<EventHandler<ValidationResult>>,
) -> Element {
    let mut validation = use_signal(|| ValidationResult::Pending);

    // Validate on content change
    use_effect(move || {
        let content = value();
        let result = if validate_schema {
            validate_package_yaml(&content)
        } else {
            match serde_yaml::from_str::<serde_yaml::Value>(&content) {
                Ok(_) => ValidationResult::Valid,
                Err(e) => ValidationResult::YamlError(format!("YAML syntax error: {}", e)),
            }
        };
        validation.set(result.clone());
        if let Some(handler) = &on_validation_change {
            handler.call(result);
        }
    });

    // Border color based on validation state
    let border_class = match validation() {
        ValidationResult::Pending => "border-gray-300 dark:border-gray-600",
        ValidationResult::Valid => "border-green-500 dark:border-green-600",
        ValidationResult::YamlError(_) | ValidationResult::SchemaError(_) => {
            "border-red-500 dark:border-red-600"
        }
    };

    let status_indicator = match &validation() {
        ValidationResult::Pending => rsx! {
            span { class: "text-gray-400", "⏳ Validating..." }
        },
        ValidationResult::Valid => rsx! {
            span { class: "text-green-600 dark:text-green-400", "✓ Valid YAML" }
        },
        ValidationResult::YamlError(msg) => rsx! {
            span { class: "text-red-600 dark:text-red-400", "✗ {msg}" }
        },
        ValidationResult::SchemaError(msg) => rsx! {
            span { class: "text-red-600 dark:text-red-400", "✗ {msg}" }
        },
    };

    let line_numbers = generate_line_numbers(&value());
    let highlighted_content = highlight_yaml(&value());
    let line_count = value().lines().count().max(1);

    // Generate unique IDs for scroll sync
    let editor_id = use_hook(|| format!("yaml-editor-{}", js_sys::Math::random().to_bits()));
    let line_id = format!("{}-lines", editor_id);
    let highlight_id = format!("{}-highlight", editor_id);
    let textarea_id = format!("{}-textarea", editor_id);

    rsx! {
        div { class: "yaml-editor",
            // Editor container with line numbers and syntax highlighting
            div { class: "yaml-editor-container {height} border-2 {border_class} rounded-md overflow-hidden transition-colors",
                // Line numbers gutter
                div {
                    id: "{line_id}",
                    class: "yaml-line-numbers",
                    {line_numbers}
                }

                // Editor area (textarea + highlight overlay)
                div { class: "yaml-editor-area",
                    // Syntax highlighted backdrop
                    pre {
                        id: "{highlight_id}",
                        class: "yaml-highlight-layer",
                        dangerous_inner_html: "{highlighted_content}\n"
                    }

                    // Actual textarea (transparent text, visible caret)
                    textarea {
                        id: "{textarea_id}",
                        class: "yaml-textarea",
                        spellcheck: "false",
                        autocomplete: "off",
                        autocapitalize: "off",
                        placeholder: "{placeholder}",
                        value: "{value}",
                        oninput: move |e| value.set(e.value()),
                        // Scroll sync via inline JS
                        onscroll: move |_| {
                            // Use web_sys to sync scroll
                            if let Some(window) = web_sys::window() {
                                if let Some(doc) = window.document() {
                                    if let (Some(textarea), Some(lines), Some(highlight)) = (
                                        doc.get_element_by_id(&textarea_id),
                                        doc.get_element_by_id(&line_id),
                                        doc.get_element_by_id(&highlight_id),
                                    ) {
                                        let scroll_top = textarea.scroll_top();
                                        lines.set_scroll_top(scroll_top);
                                        highlight.set_scroll_top(scroll_top);
                                    }
                                }
                            }
                        },
                    }
                }

                // Line count badge (top-right)
                div { class: "absolute top-2 right-2 text-xs text-gray-400 dark:text-gray-500 bg-gray-50/80 dark:bg-gray-900/80 px-1.5 py-0.5 rounded pointer-events-none z-10",
                    {format!("{} lines", line_count)}
                }
            }

            // Validation status bar
            div { class: "mt-2 flex items-center justify-between text-sm",
                {status_indicator}

                div { class: "flex gap-2",
                    button {
                        class: "px-2 py-1 text-xs text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors",
                        title: "Format YAML",
                        onclick: move |_| {
                            if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&value()) {
                                if let Ok(formatted) = serde_yaml::to_string(&val) {
                                    value.set(formatted);
                                }
                            }
                        },
                        "Format"
                    }
                    button {
                        class: "px-2 py-1 text-xs text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors",
                        title: "Clear content",
                        onclick: move |_| value.set(String::new()),
                        "Clear"
                    }
                }
            }
        }
    }
}

/// Deployment YAML Editor (validates as OClassDeployment) with line numbers and highlighting
#[component]
pub fn DeploymentYamlEditor(
    /// The YAML content signal
    value: Signal<String>,
    /// Placeholder text
    #[props(default = "Enter deployment YAML...")]
    placeholder: &'static str,
    /// Height class (default h-80)
    #[props(default = "h-80")]
    height: &'static str,
    /// Callback when validation state changes
    #[props(default)]
    on_validation_change: Option<EventHandler<ValidationResult>>,
) -> Element {
    let mut validation = use_signal(|| ValidationResult::Pending);

    // Validate on content change
    use_effect(move || {
        let content = value();
        let result = validate_deployment_yaml(&content);
        validation.set(result.clone());
        if let Some(handler) = &on_validation_change {
            handler.call(result);
        }
    });

    let border_class = match validation() {
        ValidationResult::Pending => "border-gray-300 dark:border-gray-600",
        ValidationResult::Valid => "border-green-500 dark:border-green-600",
        ValidationResult::YamlError(_) | ValidationResult::SchemaError(_) => {
            "border-red-500 dark:border-red-600"
        }
    };

    let status_indicator = match &validation() {
        ValidationResult::Pending => rsx! {
            span { class: "text-gray-400", "⏳ Validating..." }
        },
        ValidationResult::Valid => rsx! {
            span { class: "text-green-600 dark:text-green-400", "✓ Valid YAML" }
        },
        ValidationResult::YamlError(msg) => rsx! {
            span { class: "text-red-600 dark:text-red-400", "✗ {msg}" }
        },
        ValidationResult::SchemaError(msg) => rsx! {
            span { class: "text-red-600 dark:text-red-400", "✗ {msg}" }
        },
    };

    let line_numbers = generate_line_numbers(&value());
    let highlighted_content = highlight_yaml(&value());
    let line_count = value().lines().count().max(1);

    // Generate unique IDs for scroll sync
    let editor_id = use_hook(|| format!("dep-yaml-editor-{}", js_sys::Math::random().to_bits()));
    let line_id = format!("{}-lines", editor_id);
    let highlight_id = format!("{}-highlight", editor_id);
    let textarea_id = format!("{}-textarea", editor_id);

    rsx! {
        div { class: "yaml-editor",
            div { class: "yaml-editor-container {height} border-2 {border_class} rounded-md overflow-hidden transition-colors",
                // Line numbers gutter
                div {
                    id: "{line_id}",
                    class: "yaml-line-numbers",
                    {line_numbers}
                }

                // Editor area
                div { class: "yaml-editor-area",
                    // Syntax highlighted backdrop
                    pre {
                        id: "{highlight_id}",
                        class: "yaml-highlight-layer",
                        dangerous_inner_html: "{highlighted_content}\n"
                    }

                    // Actual textarea
                    textarea {
                        id: "{textarea_id}",
                        class: "yaml-textarea",
                        spellcheck: "false",
                        autocomplete: "off",
                        autocapitalize: "off",
                        placeholder: "{placeholder}",
                        value: "{value}",
                        oninput: move |e| value.set(e.value()),
                        onscroll: move |_| {
                            if let Some(window) = web_sys::window() {
                                if let Some(doc) = window.document() {
                                    if let (Some(textarea), Some(lines), Some(highlight)) = (
                                        doc.get_element_by_id(&textarea_id),
                                        doc.get_element_by_id(&line_id),
                                        doc.get_element_by_id(&highlight_id),
                                    ) {
                                        let scroll_top = textarea.scroll_top();
                                        lines.set_scroll_top(scroll_top);
                                        highlight.set_scroll_top(scroll_top);
                                    }
                                }
                            }
                        },
                    }
                }

                div { class: "absolute top-2 right-2 text-xs text-gray-400 dark:text-gray-500 bg-gray-50/80 dark:bg-gray-900/80 px-1.5 py-0.5 rounded pointer-events-none z-10",
                    {format!("{} lines", line_count)}
                }
            }

            div { class: "mt-2 flex items-center justify-between text-sm",
                {status_indicator}
                div { class: "flex gap-2",
                    button {
                        class: "px-2 py-1 text-xs text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-gray-700 rounded transition-colors",
                        onclick: move |_| {
                            if let Ok(val) = serde_yaml::from_str::<serde_yaml::Value>(&value()) {
                                if let Ok(formatted) = serde_yaml::to_string(&val) {
                                    value.set(formatted);
                                }
                            }
                        },
                        "Format"
                    }
                }
            }
        }
    }
}

/// Validate YAML content as an OClassDeployment
pub fn validate_deployment_yaml(content: &str) -> ValidationResult {
    use oprc_models::OClassDeployment;

    if content.trim().is_empty() {
        return ValidationResult::YamlError("YAML content is empty".to_string());
    }

    match serde_yaml::from_str::<serde_yaml::Value>(content) {
        Ok(_) => {}
        Err(e) => {
            return ValidationResult::YamlError(format!("YAML syntax error: {}", e));
        }
    }

    match serde_yaml::from_str::<OClassDeployment>(content) {
        Ok(dep) => {
            if dep.key.is_empty() {
                return ValidationResult::SchemaError("Deployment key is required".to_string());
            }
            if dep.package_name.is_empty() {
                return ValidationResult::SchemaError("Package name is required".to_string());
            }
            if dep.class_key.is_empty() {
                return ValidationResult::SchemaError("Class key is required".to_string());
            }
            ValidationResult::Valid
        }
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("missing field") {
                ValidationResult::SchemaError(format!("Missing required field: {}", err_str))
            } else {
                ValidationResult::SchemaError(format!("Schema validation failed: {}", err_str))
            }
        }
    }
}
