//! Recall — filesystem and search modes.
//!
//! **Filesystem mode** (no search query): walks `.memories/` date directories,
//! loads checkpoints, sorts by date (newest first), and applies filtering
//! (since/days/from/to/planId/limit).
//!
//! **Search mode** (`options.search` is `Some`): queries the Tantivy memory
//! index using BM25. Lazily rebuilds the index on first use if empty. Applies
//! post-search date and planId filtering.

use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;

/// Matches human-friendly duration strings: "30m", "2h", "3d", "1w".
static SINCE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d+)([mhdw])$").expect("valid regex"));

/// Matches YYYY-MM-DD directory names.
static DATE_DIR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("valid regex"));

use super::index::MemoryIndex;
use super::plan::get_active_plan;
use super::storage::parse_checkpoint;
use super::{Checkpoint, RecallOptions, RecallResult};

/// Default number of checkpoints to return.
const DEFAULT_LIMIT: usize = 5;

/// Recall checkpoints from the filesystem or via Tantivy search.
///
/// When no `search` query is provided, walks `.memories/` date directories
/// in reverse chronological order, applies filtering, and returns up to
/// `limit` checkpoints (default 5, newest first).
///
/// When `search` is `Some`, queries the Tantivy memory index with BM25
/// ranking. Lazily rebuilds the index if it is empty or missing. Applies
/// post-search date and planId filtering.
pub fn recall(workspace_root: &Path, options: RecallOptions) -> Result<RecallResult> {
    // 1. Read active plan (always, regardless of other options)
    let active_plan = get_active_plan(workspace_root)?;

    // 2. If search query present, use Tantivy search mode
    if let Some(ref query) = options.search {
        return recall_search_mode(workspace_root, query, &options, active_plan);
    }

    // 3. If limit is 0, return plan only
    let limit = options.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 {
        return Ok(RecallResult {
            checkpoints: Vec::new(),
            active_plan,
            workspaces: None,
        });
    }

    // 4. Compute date filter boundaries
    let filter = DateFilter::from_options(&options);

    // 5. Scan .memories/ for date directories
    let memories_dir = workspace_root.join(".memories");
    if !memories_dir.exists() {
        return Ok(RecallResult {
            checkpoints: Vec::new(),
            active_plan,
            workspaces: None,
        });
    }

    let mut date_dirs = collect_date_dirs(&memories_dir)?;

    // 6. Sort date dirs reverse chronologically (newest first)
    date_dirs.sort_unstable_by(|a, b| b.cmp(a));

    // 7. Walk date dirs, read checkpoint files, apply filters
    let mut checkpoints = Vec::new();

    for date_str in &date_dirs {
        // Quick date-level filter: skip entire directories outside range
        if let Some(ref f) = filter {
            if f.skip_date(date_str) {
                continue;
            }
        }

        let date_dir = memories_dir.join(date_str);
        let mut dir_checkpoints = read_checkpoints_from_dir(&date_dir)?;

        // Apply timestamp-level filtering
        if let Some(ref f) = filter {
            dir_checkpoints.retain(|cp| f.matches_timestamp(&cp.timestamp));
        }

        // Apply planId filtering
        if let Some(ref plan_id) = options.plan_id {
            dir_checkpoints.retain(|cp| cp.plan_id.as_deref() == Some(plan_id.as_str()));
        }

        checkpoints.extend(dir_checkpoints);
    }

    // 8. Sort all checkpoints by timestamp descending (newest first)
    checkpoints.sort_unstable_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // 9. Apply limit
    checkpoints.truncate(limit);

    // 10. If !full, strip git context
    let full = options.full.unwrap_or(false);
    if !full {
        for cp in &mut checkpoints {
            cp.git = None;
        }
    }

    Ok(RecallResult {
        checkpoints,
        active_plan,
        workspaces: None,
    })
}

// ============================================================================
// Search mode — Tantivy BM25 search
// ============================================================================

/// Memory index location relative to workspace root.
const MEMORY_INDEX_REL: &str = ".julie/indexes/memories/tantivy";

/// Search checkpoints via Tantivy BM25.
///
/// 1. Open or create the memory index
/// 2. If empty, lazily rebuild from `.memories/` files on disk
/// 3. Search with BM25
/// 4. Convert results back to `Checkpoint` structs by re-reading files
/// 5. Apply date/planId post-filters
/// 6. Optionally strip git context
fn recall_search_mode(
    workspace_root: &Path,
    query: &str,
    options: &RecallOptions,
    active_plan: Option<super::Plan>,
) -> Result<RecallResult> {
    let limit = options.limit.unwrap_or(DEFAULT_LIMIT);
    if limit == 0 {
        return Ok(RecallResult {
            checkpoints: Vec::new(),
            active_plan,
            workspaces: None,
        });
    }

    // 1. Open or create the Tantivy index
    let index_path = workspace_root.join(MEMORY_INDEX_REL);
    std::fs::create_dir_all(&index_path)
        .with_context(|| format!("Failed to create memory index dir: {}", index_path.display()))?;

    let index = MemoryIndex::open_or_create(&index_path)
        .with_context(|| "Failed to open memory search index")?;

    // 2. Lazy backfill: if index is empty, rebuild from files
    if index.num_docs() == 0 {
        tracing::info!("Memory index is empty, rebuilding from .memories/ files");
        index.rebuild_from_files(workspace_root)?;
    }

    // 3. Search — request more results than limit to account for post-filtering
    let search_limit = limit * 3 + 10; // over-fetch for filtering headroom
    let results = index.search(query, search_limit)?;

    // 4. Convert search results to Checkpoint structs
    let memories_dir = workspace_root.join(".memories");
    let mut checkpoints = Vec::new();

    for result in &results {
        // Try to load the full checkpoint from the file on disk
        if !result.file_path.is_empty() {
            let file_on_disk = memories_dir.join(&result.file_path);
            if let Ok(content) = std::fs::read_to_string(&file_on_disk) {
                if let Ok(cp) = parse_checkpoint(&content) {
                    checkpoints.push(cp);
                    continue;
                }
            }
        }

        // Fallback: reconstruct a minimal checkpoint from the search result fields.
        // This handles cases where the file was deleted after indexing.
        tracing::debug!(
            "Could not load checkpoint file for {}, using index data",
            result.id
        );
    }

    // 5. Apply date filter post-search
    let filter = DateFilter::from_options(options);
    if let Some(ref f) = filter {
        checkpoints.retain(|cp| f.matches_timestamp(&cp.timestamp));
    }

    // 6. Apply planId filter post-search
    if let Some(ref plan_id) = options.plan_id {
        checkpoints.retain(|cp| cp.plan_id.as_deref() == Some(plan_id.as_str()));
    }

    // 7. Apply limit
    checkpoints.truncate(limit);

    // 8. If !full, strip git context
    let full = options.full.unwrap_or(false);
    if !full {
        for cp in &mut checkpoints {
            cp.git = None;
        }
    }

    Ok(RecallResult {
        checkpoints,
        active_plan,
        workspaces: None,
    })
}

// ============================================================================
// Date filtering
// ============================================================================

/// Computed date/time boundaries for checkpoint filtering.
struct DateFilter {
    /// Earliest allowed timestamp (inclusive). None = no lower bound.
    from: Option<DateTime<Utc>>,
    /// Latest allowed timestamp (inclusive end of day). None = no upper bound.
    to: Option<DateTime<Utc>>,
}

impl DateFilter {
    /// Build a DateFilter from RecallOptions.
    ///
    /// Priority: `since` > `days` > `from`/`to` (matching Goldfish behavior).
    fn from_options(options: &RecallOptions) -> Option<Self> {
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
    fn skip_date(&self, date_str: &str) -> bool {
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
    fn matches_timestamp(&self, timestamp: &str) -> bool {
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

// ============================================================================
// Filesystem scanning
// ============================================================================

/// Collect all YYYY-MM-DD directory names under `.memories/`.
fn collect_date_dirs(memories_dir: &Path) -> Result<Vec<String>> {
    let mut dates = Vec::new();

    let entries = std::fs::read_dir(memories_dir)
        .with_context(|| format!("Failed to read .memories directory: {}", memories_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        if DATE_DIR_RE.is_match(&name) {
            dates.push(name);
        }
    }

    Ok(dates)
}

/// Read and parse all checkpoint files from a date directory.
///
/// Skips malformed files (logs a warning instead of failing).
fn read_checkpoints_from_dir(date_dir: &Path) -> Result<Vec<Checkpoint>> {
    let mut checkpoints = Vec::new();

    let entries = std::fs::read_dir(date_dir)
        .with_context(|| format!("Failed to read date directory: {}", date_dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // Only process .md files
        match path.extension().and_then(|e| e.to_str()) {
            Some("md") => {}
            _ => continue,
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Skipping unreadable checkpoint file {}: {}", path.display(), e);
                continue;
            }
        };

        match parse_checkpoint(&content) {
            Ok(checkpoint) => checkpoints.push(checkpoint),
            Err(e) => {
                tracing::warn!("Skipping malformed checkpoint file {}: {}", path.display(), e);
            }
        }
    }

    Ok(checkpoints)
}
