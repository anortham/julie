//! Codehealth snapshot trend formatting.
//!
//! Compares successive snapshots stored in `daemon.db` and renders the
//! results for the `"trend"` category of `query_metrics`.

use crate::daemon::database::{CodehealthSnapshot, CodehealthSnapshotRow};

/// Format a numeric delta as `"old → new (↑/↓N, ±P%)"` or `"N (no change)"`.
///
/// Used for displaying security/change risk counts across two snapshots.
pub fn format_delta(old: i64, new: i64) -> String {
    if old == new {
        return format!("{old} (no change)");
    }
    let diff = new - old;
    let arrow = if diff < 0 { '↓' } else { '↑' };
    let abs_diff = diff.unsigned_abs();

    if old == 0 {
        // Avoid division by zero; percentage is undefined
        return format!("{old} → {new} ({arrow}{abs_diff})");
    }

    let pct = (diff.unsigned_abs() as f64 / old.unsigned_abs() as f64) * 100.0;
    let sign = if diff < 0 { '-' } else { '+' };
    format!("{old} → {new} ({arrow}{abs_diff}, {sign}{pct:.0}%)")
}

/// Format a side-by-side comparison between `current` (just indexed) and
/// the most recent persisted `previous` snapshot.
pub fn format_comparison(
    current: &CodehealthSnapshot,
    previous: &CodehealthSnapshotRow,
) -> String {
    let mut lines = Vec::new();

    lines.push("Codehealth Trend (vs previous snapshot)".to_string());
    lines.push(String::new());

    lines.push(format!(
        "Security Risk:   HIGH {}  |  MED {}  |  LOW {}",
        format_delta(previous.security_high as i64, current.security_high as i64),
        format_delta(
            previous.security_medium as i64,
            current.security_medium as i64
        ),
        format_delta(previous.security_low as i64, current.security_low as i64),
    ));

    lines.push(format!(
        "Change Risk:     HIGH {}  |  MED {}  |  LOW {}",
        format_delta(previous.change_high as i64, current.change_high as i64),
        format_delta(
            previous.change_medium as i64,
            current.change_medium as i64
        ),
        format_delta(previous.change_low as i64, current.change_low as i64),
    ));

    lines.push(format!(
        "Test Coverage:   tested {}  |  untested {}",
        format_delta(previous.symbols_tested, current.symbols_tested),
        format_delta(previous.symbols_untested, current.symbols_untested),
    ));

    lines.push(format!(
        "Symbols:         {}",
        format_delta(previous.total_symbols, current.total_symbols),
    ));

    lines.join("\n")
}

/// Format a historical table of snapshots (newest first).
pub fn format_trend_table(snapshots: &[CodehealthSnapshotRow]) -> String {
    if snapshots.is_empty() {
        return "No codehealth history available yet.".to_string();
    }

    let header = format!(
        "{:<12}  {:>8}  {:>8}  {:>7}  {:>8}  {:>7}  {:>8}",
        "Date", "Symbols", "Sec HIGH", "Sec MED", "Chg HIGH", "Tested", "Untested"
    );
    let separator = "-".repeat(header.len());

    let mut lines = vec![header, separator];

    for snap in snapshots {
        let date = timestamp_to_date(snap.timestamp);
        lines.push(format!(
            "{:<12}  {:>8}  {:>8}  {:>7}  {:>8}  {:>7}  {:>8}",
            date,
            snap.total_symbols,
            snap.security_high,
            snap.security_medium,
            snap.change_high,
            snap.symbols_tested,
            snap.symbols_untested,
        ));
    }

    lines.join("\n")
}

/// Convert a Unix timestamp to a `"YYYY-MM-DD"` string.
///
/// Uses a manual Gregorian calendar calculation to avoid pulling in the
/// `chrono` crate for this simple display-only function.
fn timestamp_to_date(ts: i64) -> String {
    if ts <= 0 {
        return "unknown".to_string();
    }

    let mut remaining = ts / 86400; // days since 1970-01-01
    let mut year = 1970i64;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }

    let month_days: [i64; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1i64;
    for &m in &month_days {
        if remaining < m {
            break;
        }
        remaining -= m;
        month += 1;
    }
    let day = remaining + 1;

    format!("{year:04}-{month:02}-{day:02}")
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}
