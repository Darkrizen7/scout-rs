//! Local date and time text.

use chrono::{Local, TimeZone};

/// "21/11/2026 12:00" in local time; empty for an impossible timestamp.
pub fn format_datetime(ts: i64) -> String {
    format_local(ts, "%d/%m/%Y %H:%M")
}

/// "21/11 12h", short enough for a chart axis.
pub fn format_axis(ts: i64) -> String {
    format_local(ts, "%d/%m %Hh")
}

fn format_local(ts: i64, pattern: &str) -> String {
    Local
        .timestamp_opt(ts, 0)
        .single()
        .map(|d| d.format(pattern).to_string())
        .unwrap_or_default()
}
