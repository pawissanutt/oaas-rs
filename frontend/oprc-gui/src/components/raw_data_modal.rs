//! Raw data modal component for viewing JSON/YAML with copy functionality

use dioxus::prelude::*;
use serde::Serialize;
use wasm_bindgen_futures::JsFuture;

/// Format options for raw data display
#[derive(Clone, Copy, PartialEq, Default)]
pub enum DataFormat {
    #[default]
    Json,
    Yaml,
}

/// Apply syntax highlighting to JSON/YAML content
fn highlight_syntax(content: &str, format: DataFormat) -> String {
    let mut result = String::with_capacity(content.len() * 2);

    match format {
        DataFormat::Json => {
            let mut chars = content.chars().peekable();
            while let Some(c) = chars.next() {
                match c {
                    '"' => {
                        // Start of a string
                        let mut s = String::from("\"");
                        let mut is_key = false;
                        while let Some(&next) = chars.peek() {
                            s.push(chars.next().unwrap());
                            if next == '"' {
                                break;
                            }
                            if next == '\\' {
                                if let Some(escaped) = chars.next() {
                                    s.push(escaped);
                                }
                            }
                        }
                        // Check if this is a key (followed by colon)
                        let mut temp_chars = chars.clone();
                        while let Some(&ws) = temp_chars.peek() {
                            if ws.is_whitespace() {
                                temp_chars.next();
                            } else {
                                break;
                            }
                        }
                        if temp_chars.peek() == Some(&':') {
                            is_key = true;
                        }

                        let escaped = html_escape(&s);
                        if is_key {
                            result.push_str(&format!(
                                "<span class=\"text-purple-600 dark:text-purple-400\">{}</span>",
                                escaped
                            ));
                        } else {
                            result.push_str(&format!(
                                "<span class=\"text-green-600 dark:text-green-400\">{}</span>",
                                escaped
                            ));
                        }
                    }
                    ':' => {
                        result.push_str("<span class=\"text-gray-600 dark:text-gray-400\">:</span>");
                    }
                    '{' | '}' | '[' | ']' => {
                        result.push_str(&format!(
                            "<span class=\"text-gray-700 dark:text-gray-300\">{}</span>",
                            c
                        ));
                    }
                    '0'..='9' | '-' | '.' => {
                        // Number
                        let mut num = String::from(c);
                        while let Some(&next) = chars.peek() {
                            if next.is_ascii_digit() || next == '.' || next == 'e' || next == 'E' || next == '+' || next == '-' {
                                num.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        // Check if it's actually a number or just a minus sign
                        if num == "-" || num.parse::<f64>().is_err() {
                            result.push_str(&num);
                        } else {
                            result.push_str(&format!(
                                "<span class=\"text-blue-600 dark:text-blue-400\">{}</span>",
                                num
                            ));
                        }
                    }
                    't' | 'f' => {
                        // true/false
                        let mut word = String::from(c);
                        while let Some(&next) = chars.peek() {
                            if next.is_alphabetic() {
                                word.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        if word == "true" || word == "false" {
                            result.push_str(&format!(
                                "<span class=\"text-orange-600 dark:text-orange-400\">{}</span>",
                                word
                            ));
                        } else {
                            result.push_str(&word);
                        }
                    }
                    'n' => {
                        // null
                        let mut word = String::from(c);
                        while let Some(&next) = chars.peek() {
                            if next.is_alphabetic() {
                                word.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                        if word == "null" {
                            result.push_str(&format!(
                                "<span class=\"text-red-600 dark:text-red-400\">{}</span>",
                                word
                            ));
                        } else {
                            result.push_str(&word);
                        }
                    }
                    _ => {
                        if c == '<' {
                            result.push_str("&lt;");
                        } else if c == '>' {
                            result.push_str("&gt;");
                        } else if c == '&' {
                            result.push_str("&amp;");
                        } else {
                            result.push(c);
                        }
                    }
                }
            }
        }
        DataFormat::Yaml => {
            for line in content.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with('#') {
                    // Comment
                    result.push_str(&format!(
                        "<span class=\"text-gray-500 dark:text-gray-500\">{}</span>\n",
                        html_escape(line)
                    ));
                } else if let Some(colon_pos) = line.find(':') {
                    let (key, rest) = line.split_at(colon_pos);
                    result.push_str(&format!(
                        "<span class=\"text-purple-600 dark:text-purple-400\">{}</span>",
                        html_escape(key)
                    ));
                    result.push_str("<span class=\"text-gray-600 dark:text-gray-400\">:</span>");
                    let value = &rest[1..]; // skip the colon
                    let value_trimmed = value.trim();
                    if value_trimmed == "true" || value_trimmed == "false" {
                        let spaces = &value[..value.len() - value.trim_start().len()];
                        result.push_str(spaces);
                        result.push_str(&format!(
                            "<span class=\"text-orange-600 dark:text-orange-400\">{}</span>",
                            value_trimmed
                        ));
                    } else if value_trimmed == "null" || value_trimmed == "~" {
                        let spaces = &value[..value.len() - value.trim_start().len()];
                        result.push_str(spaces);
                        result.push_str(&format!(
                            "<span class=\"text-red-600 dark:text-red-400\">{}</span>",
                            value_trimmed
                        ));
                    } else if value_trimmed.parse::<f64>().is_ok() {
                        let spaces = &value[..value.len() - value.trim_start().len()];
                        result.push_str(spaces);
                        result.push_str(&format!(
                            "<span class=\"text-blue-600 dark:text-blue-400\">{}</span>",
                            value_trimmed
                        ));
                    } else if (value_trimmed.starts_with('"') && value_trimmed.ends_with('"'))
                        || (value_trimmed.starts_with('\'') && value_trimmed.ends_with('\''))
                    {
                        let spaces = &value[..value.len() - value.trim_start().len()];
                        result.push_str(spaces);
                        result.push_str(&format!(
                            "<span class=\"text-green-600 dark:text-green-400\">{}</span>",
                            html_escape(value_trimmed)
                        ));
                    } else {
                        result.push_str(&html_escape(value));
                    }
                    result.push('\n');
                } else if trimmed.starts_with('-') {
                    // List item
                    let indent = &line[..line.len() - trimmed.len()];
                    result.push_str(indent);
                    result.push_str("<span class=\"text-gray-600 dark:text-gray-400\">-</span>");
                    result.push_str(&html_escape(&trimmed[1..]));
                    result.push('\n');
                } else {
                    result.push_str(&html_escape(line));
                    result.push('\n');
                }
            }
            // Remove trailing newline
            if result.ends_with('\n') {
                result.pop();
            }
        }
    }

    result
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// A modal component that displays raw JSON/YAML data with copy functionality
#[component]
pub fn RawDataModal(
    /// Whether the modal is visible
    show: Signal<bool>,
    /// The JSON string to display (pre-serialized)
    json_data: String,
    /// Title for the modal
    #[props(default = "Raw Data")]
    title: &'static str,
) -> Element {
    let mut format = use_signal(|| DataFormat::Json);
    let mut copied = use_signal(|| false);

    // Format the data based on selected format
    let formatted_content = use_memo(move || {
        let json_str = json_data.clone();
        match format() {
            DataFormat::Json => {
                // Try to pretty-print the JSON
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    serde_json::to_string_pretty(&value).unwrap_or(json_str)
                } else {
                    json_str
                }
            }
            DataFormat::Yaml => {
                // Convert JSON to YAML
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    serde_yaml::to_string(&value).unwrap_or(json_str)
                } else {
                    json_str
                }
            }
        }
    });

    // Highlighted content
    let highlighted_content = use_memo(move || {
        highlight_syntax(&formatted_content(), format())
    });

    // Copy to clipboard handler
    let copy_handler = move |_| {
        let content = formatted_content();
        spawn(async move {
            if let Some(window) = web_sys::window() {
                let clipboard = window.navigator().clipboard();
                let _ = JsFuture::from(clipboard.write_text(&content)).await;
                copied.set(true);
                // Reset copied state after 2 seconds
                gloo_timers::future::TimeoutFuture::new(2000).await;
                copied.set(false);
            }
        });
    };

    if !show() {
        return rsx! {};
    }

    let content = formatted_content();
    let line_count = content.lines().count();
    let highlighted = highlighted_content();

    rsx! {
        // Modal backdrop
        div {
            class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
            onclick: move |_| show.set(false),

            // Modal content
            div {
                class: "bg-white dark:bg-gray-800 rounded-lg shadow-xl max-w-4xl w-full mx-4 max-h-[90vh] flex flex-col",
                onclick: move |e| e.stop_propagation(),

                // Header
                div { class: "flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-gray-700",
                    h3 { class: "text-lg font-semibold text-gray-900 dark:text-gray-100",
                        "{title}"
                    }

                    div { class: "flex items-center gap-3",
                        // Format toggle
                        div { class: "flex rounded-md overflow-hidden border border-gray-300 dark:border-gray-600",
                            button {
                                class: if format() == DataFormat::Json {
                                    "px-3 py-1 text-sm bg-blue-600 text-white"
                                } else {
                                    "px-3 py-1 text-sm bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600"
                                },
                                onclick: move |_| format.set(DataFormat::Json),
                                "JSON"
                            }
                            button {
                                class: if format() == DataFormat::Yaml {
                                    "px-3 py-1 text-sm bg-blue-600 text-white"
                                } else {
                                    "px-3 py-1 text-sm bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600"
                                },
                                onclick: move |_| format.set(DataFormat::Yaml),
                                "YAML"
                            }
                        }

                        // Copy button
                        button {
                            class: "flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-md transition-colors",
                            class: if copied() {
                                "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300"
                            } else {
                                "bg-gray-100 dark:bg-gray-700 text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-600"
                            },
                            onclick: copy_handler,
                            if copied() {
                                "✓ Copied!"
                            } else {
                                "📋 Copy"
                            }
                        }

                        // Close button
                        button {
                            class: "p-1 text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200",
                            onclick: move |_| show.set(false),
                            "✕"
                        }
                    }
                }

                // Content area with line numbers
                div { class: "flex-1 overflow-auto",
                    div { class: "flex font-mono text-sm",
                        // Line numbers
                        div { class: "flex-shrink-0 px-3 py-4 text-right text-gray-400 dark:text-gray-500 bg-gray-50 dark:bg-gray-900 select-none border-r border-gray-200 dark:border-gray-700",
                            {(1..=line_count).map(|n| rsx! {
                                div { key: "{n}", "{n}" }
                            })}
                        }

                        // Code content with syntax highlighting
                        pre {
                            class: "flex-1 px-4 py-4 overflow-x-auto whitespace-pre",
                            dangerous_inner_html: "{highlighted}"
                        }
                    }
                }

                // Footer
                div { class: "px-6 py-3 border-t border-gray-200 dark:border-gray-700 text-sm text-gray-500 dark:text-gray-400",
                    "{line_count} lines"
                }
            }
        }
    }
}

/// Helper function to serialize data to JSON for the modal
pub fn to_json_string<T: Serialize>(data: &T) -> String {
    serde_json::to_string(data).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}
