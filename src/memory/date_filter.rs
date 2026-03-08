//! Date filtering for recall operations.
//!
//! Parses human-friendly duration strings ("2h", "30m", "3d", "1w"),
//! ISO 8601 timestamps, and date ranges. Used by both filesystem-mode
//! and search-mode recall to filter checkpoints by time.

use std::sync::LazyLock;

use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;

use super::RecallOptions;

/// Matches human-friendly duration strings: "30m", "2h", "3d", "1w".
static SINCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)([mhdw])$").expect("valid regex"));

/// Matches YYYY-MM-DD directory names.
pub(super) static DATE_DIR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid regex"));

/// Computed date/time boundaries for checkpoint filtering.
pub(super) struct DateFilter {
    /// Earliest allowed timestamp (inclusive). None = no lower bound.
    from: Option<DateTime<Utc>>,
    /// Latest allowed timestamp (inclusive end of day). None = no upper bound.
    to: Option<DateTime<Utc>>,
}

impl DateFilter {
    /// Build a DateFilter from RecallOptions.
    ///
    /// Priority: `since` > `days` > `from`/`to` (matching Goldfish behavior).
    pub(super) fn from_options(options: &RecallOptions) -> Option<Self> {
        // `since` takes priority
        if let Some(ref since) = options.since {
            if let Some(dt) = parse_since(since) {
                return Some(DateFilter {
                    from: Some(dt),
                    to: None,
                });
            }
        }

        // `days` is next
        if let Some(days) = options.days {
            let from = Utc::now() - chrono::Duration::days(days as i64);
            return Some(DateFilter {
                from: Some(from),
                to: None,
            });
        }

        // `from`/`to` explicit range
        let from_dt = options.from.as_ref().and_then(|s| parse_date_boundary(s, false));
        let to_dt = options.to.as_ref().and_then(|s| parse_date_boundary(s, true));

        if from_dt.is_some() || to_dt.is_some() {
            return Some(DateFilter {
                from: from_dt,
                to: to_dt,
            });
        }

        None
    }

    /// Quick check: can we skip an entire date directory?
    ///
    /// Uses date-level granularity to avoid reading files in dirs
    /// that are entirely outside the filter range.
    pub(super) fn skip_date(&self, date_str: &str) -> bool {
        let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
            return false; // Don't skip unparseable — let file-level filter handle it
        };

        // If the entire day is before our `from` boundary, skip it
        if let Some(ref from) = self.from {
            let end_of_day = date
                .and_hms_opt(23, 59, 59)
                .unwrap()
                .and_utc();
            if end_of_day < *from {
                return true;
            }
        }

        // If the entire day is after our `to` boundary, skip it
        if let Some(ref to) = self.to {
            let start_of_day = date
                .and_hms_opt(0, 0, 0)
                .unwrap()
                .and_utc();
            if start_of_day > *to {
                return true;
            }
        }

        false
    }

    /// Check if a specific checkpoint timestamp is within the filter range.
    pub(super) fn matches_timestamp(&self, timestamp: &str) -> bool {
        let Ok(ts) = DateTime::parse_from_rfc3339(timestamp) else {
            // Try a more lenient parse for non-standard timestamps
            return true; // Don't filter out unparseable timestamps
        };
        let ts = ts.with_timezone(&Utc);

        if let Some(ref from) = self.from {
            if ts < *from {
                return false;
            }
        }

        if let Some(ref to) = self.to {
            if ts > *to {
                return false;
            }
        }

        true
    }
}

/// Parse a `since` value into a UTC datetime.
///
/// Supports Goldfish-compatible duration strings:
/// - "2h" -> 2 hours ago
/// - "30m" -> 30 minutes ago
/// - "3d" -> 3 days ago
/// - "1w" -> 1 week ago
/// - ISO 8601 timestamp -> parse directly
pub fn parse_since(since: &str) -> Option<DateTime<Utc>> {
    let since = since.trim();
    if since.is_empty() {
        return None;
    }

    // Try duration format: <number><unit>
    if let Some(caps) = SINCE_RE.captures(since) {
        let amount: i64 = caps[1].parse().ok()?;
        let duration = match &caps[2] {
            "m" => chrono::Duration::minutes(amount),
            "h" => chrono::Duration::hours(amount),
            "d" => chrono::Duration::days(amount),
            "w" => chrono::Duration::weeks(amount),
            _ => return None,
        };
        return Some(Utc::now() - duration);
    }

    // Try ISO 8601 timestamp
    DateTime::parse_from_rfc3339(since)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
}

/// Parse a date boundary string into a UTC datetime.
///
/// Accepts:
/// - "YYYY-MM-DD" date string (start or end of day depending on `end_of_day`)
/// - ISO 8601 timestamp (used directly)
fn parse_date_boundary(s: &str, end_of_day: bool) -> Option<DateTime<Utc>> {
    let s = s.trim();

    // Try as ISO 8601 timestamp first
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }

    // Try as YYYY-MM-DD date
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let time = if end_of_day {
            date.and_hms_opt(23, 59, 59)?.and_utc()
        } else {
            date.and_hms_opt(0, 0, 0)?.and_utc()
        };
        return Some(time);
    }

    None
}
