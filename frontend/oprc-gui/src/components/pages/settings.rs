//! Settings page component - GUI preferences and configuration

use dioxus::prelude::*;

/// User preferences stored in localStorage
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GuiSettings {
    pub theme: Theme,
    pub default_class: Option<String>,
    pub page_size: u32,
    pub auto_refresh: bool,
    pub refresh_interval_secs: u32,
    pub sidebar_collapsed: bool,
}

#[derive(
    Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default,
)]
pub enum Theme {
    Light,
    #[default]
    Dark,
    System,
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Theme::Light => write!(f, "Light"),
            Theme::Dark => write!(f, "Dark"),
            Theme::System => write!(f, "System"),
        }
    }
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            theme: Theme::Dark,
            default_class: None,
            page_size: 25,
            auto_refresh: false,
            refresh_interval_secs: 30,
            sidebar_collapsed: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
const SETTINGS_KEY: &str = "oprc_gui_settings";

/// Load settings from localStorage
pub fn load_settings() -> GuiSettings {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(json)) = storage.get_item(SETTINGS_KEY) {
                    if let Ok(settings) = serde_json::from_str(&json) {
                        return settings;
                    }
                }
            }
        }
    }
    GuiSettings::default()
}

/// Save settings to localStorage
pub fn save_settings(settings: &GuiSettings) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(json) = serde_json::to_string(settings) {
                    let _ = storage.set_item(SETTINGS_KEY, &json);
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = settings; // Suppress unused warning
    }
}

/// Apply the theme to the document (adds/removes 'dark' class on <html>)
pub fn apply_theme(theme: &Theme) {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(window) = web_sys::window() {
            if let Some(document) = window.document() {
                if let Some(html) = document.document_element() {
                    let class_list = html.class_list();
                    match theme {
                        Theme::Dark => {
                            let _ = class_list.add_1("dark");
                        }
                        Theme::Light => {
                            let _ = class_list.remove_1("dark");
                        }
                        Theme::System => {
                            // Check system preference
                            let prefers_dark = window
                                .match_media("(prefers-color-scheme: dark)")
                                .ok()
                                .flatten()
                                .map(|mq| mq.matches())
                                .unwrap_or(true); // Default to dark if can't detect
                            if prefers_dark {
                                let _ = class_list.add_1("dark");
                            } else {
                                let _ = class_list.remove_1("dark");
                            }
                        }
                    }
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = theme; // Suppress unused warning
    }
}

#[component]
pub fn Settings() -> Element {
    // Load initial settings
    let mut settings = use_signal(load_settings);
    let mut save_status = use_signal(|| None::<String>);

    // Connection info (read-only)
    let api_base = crate::config::get_api_base_url();

    // Save handler
    let save_handler = move |_| {
        save_settings(&settings());
        save_status.set(Some("Settings saved!".to_string()));
        // Clear status after 2 seconds
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(2000).await;
            save_status.set(None);
        });
    };

    // Reset handler
    let reset_handler = move |_| {
        let defaults = GuiSettings::default();
        apply_theme(&defaults.theme);
        save_settings(&defaults);
        settings.set(defaults);
        save_status.set(Some("Settings reset to defaults".to_string()));
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(2000).await;
            save_status.set(None);
        });
    };

    rsx! {
        div { class: "container mx-auto p-6",
            h1 { class: "text-2xl font-bold mb-6 text-gray-900 dark:text-gray-100", "⚙️ Settings" }

            // Save status notification
            if let Some(status) = save_status() {
                div { class: "mb-4 px-4 py-2 bg-green-100 dark:bg-green-900/30 text-green-800 dark:text-green-300 rounded",
                    "{status}"
                }
            }

            // Appearance Section
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6 mb-6",
                h2 { class: "text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100", "Appearance" }

                div { class: "space-y-4",
                    // Theme selector
                    div { class: "flex items-center justify-between",
                        label { class: "text-gray-700 dark:text-gray-300", "Theme" }
                        select {
                            class: "px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            value: "{settings().theme}",
                            onchange: move |e| {
                                let mut s = settings();
                                s.theme = match e.value().as_str() {
                                    "Light" => Theme::Light,
                                    "System" => Theme::System,
                                    _ => Theme::Dark,
                                };
                                // Apply theme immediately
                                apply_theme(&s.theme);
                                // Save to localStorage
                                save_settings(&s);
                                settings.set(s);
                            },
                            option { value: "Dark", "🌙 Dark" }
                            option { value: "Light", "☀️ Light" }
                            option { value: "System", "💻 System" }
                        }
                    }

                    // Sidebar collapsed toggle
                    div { class: "flex items-center justify-between",
                        label { class: "text-gray-700 dark:text-gray-300", "Sidebar" }
                        div { class: "flex gap-2",
                            button {
                                class: if !settings().sidebar_collapsed {
                                    "px-3 py-1 rounded bg-blue-600 text-white"
                                } else {
                                    "px-3 py-1 rounded bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300"
                                },
                                onclick: move |_| {
                                    let mut s = settings();
                                    s.sidebar_collapsed = false;
                                    settings.set(s);
                                },
                                "Expanded"
                            }
                            button {
                                class: if settings().sidebar_collapsed {
                                    "px-3 py-1 rounded bg-blue-600 text-white"
                                } else {
                                    "px-3 py-1 rounded bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300"
                                },
                                onclick: move |_| {
                                    let mut s = settings();
                                    s.sidebar_collapsed = true;
                                    settings.set(s);
                                },
                                "Collapsed"
                            }
                        }
                    }
                }
            }

            // Defaults Section
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6 mb-6",
                h2 { class: "text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100", "Defaults" }

                div { class: "space-y-4",
                    // Default class (text input for now, could be dropdown later)
                    div { class: "flex items-center justify-between",
                        label { class: "text-gray-700 dark:text-gray-300", "Default Class" }
                        input {
                            class: "px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100 w-64",
                            r#type: "text",
                            placeholder: "e.g., example.SimpleRecord",
                            value: "{settings().default_class.clone().unwrap_or_default()}",
                            oninput: move |e| {
                                let mut s = settings();
                                s.default_class = if e.value().is_empty() { None } else { Some(e.value()) };
                                settings.set(s);
                            }
                        }
                    }

                    // Page size
                    div { class: "flex items-center justify-between",
                        label { class: "text-gray-700 dark:text-gray-300", "Page Size" }
                        select {
                            class: "px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                            value: "{settings().page_size}",
                            onchange: move |e| {
                                let mut s = settings();
                                s.page_size = e.value().parse().unwrap_or(25);
                                settings.set(s);
                            },
                            option { value: "10", "10" }
                            option { value: "25", "25" }
                            option { value: "50", "50" }
                            option { value: "100", "100" }
                        }
                    }

                    // Auto-refresh toggle
                    div { class: "flex items-center justify-between",
                        label { class: "text-gray-700 dark:text-gray-300", "Auto-refresh" }
                        div { class: "flex items-center gap-4",
                            div { class: "flex gap-2",
                                button {
                                    class: if settings().auto_refresh {
                                        "px-3 py-1 rounded bg-blue-600 text-white"
                                    } else {
                                        "px-3 py-1 rounded bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300"
                                    },
                                    onclick: move |_| {
                                        let mut s = settings();
                                        s.auto_refresh = true;
                                        settings.set(s);
                                    },
                                    "On"
                                }
                                button {
                                    class: if !settings().auto_refresh {
                                        "px-3 py-1 rounded bg-blue-600 text-white"
                                    } else {
                                        "px-3 py-1 rounded bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300"
                                    },
                                    onclick: move |_| {
                                        let mut s = settings();
                                        s.auto_refresh = false;
                                        settings.set(s);
                                    },
                                    "Off"
                                }
                            }
                            if settings().auto_refresh {
                                select {
                                    class: "px-3 py-2 border border-gray-300 dark:border-gray-600 rounded-md bg-white dark:bg-gray-700 text-gray-900 dark:text-gray-100",
                                    value: "{settings().refresh_interval_secs}",
                                    onchange: move |e| {
                                        let mut s = settings();
                                        s.refresh_interval_secs = e.value().parse().unwrap_or(30);
                                        settings.set(s);
                                    },
                                    option { value: "10", "10s" }
                                    option { value: "30", "30s" }
                                    option { value: "60", "60s" }
                                    option { value: "120", "2min" }
                                }
                            }
                        }
                    }
                }
            }

            // Connection Info Section (Read-only)
            div { class: "bg-white dark:bg-gray-800 rounded-lg shadow p-6 mb-6",
                h2 { class: "text-lg font-semibold mb-4 text-gray-900 dark:text-gray-100", "Connection Info" }
                p { class: "text-sm text-gray-500 dark:text-gray-400 mb-4", "Read-only information about the current connection" }

                div { class: "space-y-3",
                    div { class: "flex items-center justify-between",
                        span { class: "text-gray-600 dark:text-gray-400", "API Endpoint" }
                        div { class: "flex items-center gap-2",
                            code { class: "px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm text-gray-800 dark:text-gray-200",
                                "{api_base}"
                            }
                            button {
                                class: "p-1 text-gray-500 hover:text-gray-700 dark:hover:text-gray-300",
                                title: "Copy to clipboard",
                                onclick: move |_| {
                                    #[cfg(target_arch = "wasm32")]
                                    {
                                        if let Some(window) = web_sys::window() {
                                            let clipboard = window.navigator().clipboard();
                                            let _ = clipboard.write_text(&api_base);
                                        }
                                    }
                                },
                                "📋"
                            }
                        }
                    }

                    div { class: "flex items-center justify-between",
                        span { class: "text-gray-600 dark:text-gray-400", "Version" }
                        code { class: "px-2 py-1 bg-gray-100 dark:bg-gray-700 rounded text-sm text-gray-800 dark:text-gray-200",
                            "{env!(\"CARGO_PKG_VERSION\")}"
                        }
                    }
                }
            }

            // Action buttons
            div { class: "flex justify-end gap-4",
                button {
                    class: "px-4 py-2 bg-gray-200 dark:bg-gray-600 text-gray-700 dark:text-gray-300 rounded-md hover:bg-gray-300 dark:hover:bg-gray-500 transition-colors",
                    onclick: reset_handler,
                    "Reset to Defaults"
                }
                button {
                    class: "px-6 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 transition-colors",
                    onclick: save_handler,
                    "Save"
                }
            }
        }
    }
}
