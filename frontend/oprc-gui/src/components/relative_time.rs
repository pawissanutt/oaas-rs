//! Relative time component that updates in real-time

use chrono::{DateTime, Utc};
use dioxus::prelude::*;

/// Calculate relative time string from a DateTime
fn calc_relative(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);

    if duration.num_seconds() < 0 {
        "in the future".to_string()
    } else if duration.num_seconds() < 60 {
        format!("{}s ago", duration.num_seconds())
    } else if duration.num_minutes() < 60 {
        format!("{}m ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h ago", duration.num_hours())
    } else if duration.num_days() < 30 {
        format!("{}d ago", duration.num_days())
    } else if duration.num_days() < 365 {
        format!("{}mo ago", duration.num_days() / 30)
    } else {
        format!("{}y ago", duration.num_days() / 365)
    }
}

/// Calculate update interval based on how old the timestamp is
fn calc_interval_ms(dt: &DateTime<Utc>) -> u32 {
    let now = Utc::now();
    let duration = now.signed_duration_since(*dt);

    if duration.num_seconds() < 60 {
        1000 // Update every second for recent times
    } else if duration.num_minutes() < 60 {
        10_000 // Update every 10 seconds for minutes
    } else if duration.num_hours() < 24 {
        60_000 // Update every minute for hours
    } else {
        300_000 // Update every 5 minutes for days+
    }
}

/// A component that displays relative time and updates automatically
#[component]
pub fn RelativeTime(
    /// The timestamp as RFC3339 string
    timestamp: String,
    /// Optional prefix text (e.g., "Updated ", "Created: ")
    #[props(default = "")]
    prefix: &'static str,
    /// Optional CSS class
    #[props(default = "")]
    class: &'static str,
) -> Element {
    let parsed = DateTime::parse_from_rfc3339(&timestamp)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));

    let mut relative_text = use_signal(|| {
        parsed
            .as_ref()
            .map(calc_relative)
            .unwrap_or_else(|| timestamp.clone())
    });

    // Set up interval to update the relative time
    use_effect(move || {
        if let Some(dt) = parsed {
            let interval_ms = calc_interval_ms(&dt);

            spawn(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(interval_ms).await;
                    relative_text.set(calc_relative(&dt));
                }
            });
        }
    });

    let short_ts = parsed
        .as_ref()
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| timestamp.clone());

    let display_text = format!("{} ({})", relative_text(), short_ts);

    rsx! {
        span {
            class: "{class}",
            title: "{timestamp}",
            "{prefix}{display_text}"
        }
    }
}

/// A component that displays relative time from DateTime<Utc> and updates automatically
#[component]
pub fn RelativeTimeFromDt(
    /// The DateTime<Utc> timestamp
    datetime: DateTime<Utc>,
    /// Optional prefix text (e.g., "Updated ", "Created: ")
    #[props(default = "")]
    prefix: &'static str,
    /// Optional CSS class
    #[props(default = "")]
    class: &'static str,
) -> Element {
    let mut relative_text = use_signal(|| calc_relative(&datetime));

    // Set up interval to update the relative time
    use_effect(move || {
        let interval_ms = calc_interval_ms(&datetime);

        spawn(async move {
            loop {
                gloo_timers::future::TimeoutFuture::new(interval_ms).await;
                relative_text.set(calc_relative(&datetime));
            }
        });
    });

    let short_ts = datetime.format("%Y-%m-%d %H:%M:%S").to_string();
    let full_ts = datetime.to_rfc3339();
    let display_text = format!("{} ({})", relative_text(), short_ts);

    rsx! {
        span {
            class: "{class}",
            title: "{full_ts}",
            "{prefix}{display_text}"
        }
    }
}

/// A component that displays relative time from protobuf Timestamp and updates automatically
#[component]
pub fn RelativeTimeFromProto(
    /// The protobuf timestamp seconds
    seconds: i64,
    /// The protobuf timestamp nanos
    #[props(default = 0)]
    nanos: i32,
    /// Optional prefix text (e.g., "Updated ", "Created: ")
    #[props(default = "")]
    prefix: &'static str,
    /// Optional CSS class
    #[props(default = "")]
    class: &'static str,
) -> Element {
    let datetime = DateTime::from_timestamp(seconds, nanos as u32);

    let mut relative_text = use_signal(|| {
        datetime
            .as_ref()
            .map(calc_relative)
            .unwrap_or_else(|| format!("{}", seconds))
    });

    // Set up interval to update the relative time
    use_effect(move || {
        if let Some(dt) = datetime {
            let interval_ms = calc_interval_ms(&dt);

            spawn(async move {
                loop {
                    gloo_timers::future::TimeoutFuture::new(interval_ms).await;
                    relative_text.set(calc_relative(&dt));
                }
            });
        }
    });

    let short_ts = datetime
        .as_ref()
        .map(|dt| dt.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| format!("{}", seconds));

    let full_ts = datetime
        .as_ref()
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| format!("{}", seconds));

    let display_text = format!("{} ({})", relative_text(), short_ts);

    rsx! {
        span {
            class: "{class}",
            title: "{full_ts}",
            "{prefix}{display_text}"
        }
    }
}
