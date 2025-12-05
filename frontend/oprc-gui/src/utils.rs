//! Utility functions for the GUI

use chrono::{DateTime, Utc};

/// Format a timestamp string as relative time (e.g., "5s ago", "2h ago")
/// with the original timestamp shown in shortened form
pub fn format_relative_time(timestamp: &str) -> String {
    // Try to parse as RFC3339/ISO8601
    if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) {
        let now = Utc::now();
        let dt_utc = dt.with_timezone(&Utc);
        format_relative_from_dt(&dt_utc, &now)
    } else {
        // If parsing fails, just return original
        timestamp.to_string()
    }
}

/// Format a DateTime<Utc> as relative time
pub fn format_relative_from_datetime(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    format_relative_from_dt(dt, &now)
}

fn format_relative_from_dt(dt: &DateTime<Utc>, now: &DateTime<Utc>) -> String {
    let duration = now.signed_duration_since(*dt);

    let relative = if duration.num_seconds() < 0 {
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
    };

    // Shorten the timestamp: show date + time without timezone/milliseconds
    let short_ts = dt.format("%Y-%m-%d %H:%M:%S").to_string();
    format!("{} ({})", relative, short_ts)
}

/// Format a timestamp string as just relative time (e.g., "5s ago")
pub fn format_relative_only(timestamp: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) {
        let now = Utc::now();
        let dt_utc = dt.with_timezone(&Utc);
        let duration = now.signed_duration_since(dt_utc);

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
    } else {
        timestamp.to_string()
    }
}

/// Shorten a timestamp to just date and time
pub fn shorten_timestamp(timestamp: &str) -> String {
    if let Ok(dt) = DateTime::parse_from_rfc3339(timestamp) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        timestamp.to_string()
    }
}
