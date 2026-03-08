//! Recall — filesystem, search, and cross-project modes.
//!
//! **Filesystem mode** (no search query): walks `.memories/` date directories,
//! loads checkpoints, sorts by date (newest first), and applies filtering
//! (since/days/from/to/planId/limit).
//!
//! **Search mode** (`options.search` is `Some`): queries the Tantivy memory
//! index using BM25. Lazily rebuilds the index on first use if empty. Applies
//! post-search date and planId filtering.
//!
//! **Cross-project mode**: aggregates checkpoints from multiple workspaces,
//! tags each with its source project name, and builds workspace summaries.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::date_filter::{DateFilter, DATE_DIR_RE};
use super::index::MemoryIndex;
use super::plan::get_active_plan;
use super::storage::parse_checkpoint;
use super::{Checkpoint, RecallOptions, RecallResult};

// Re-export parse_since for use by tests and other modules.
pub use super::date_filter::parse_since;

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
// Cross-project recall (daemon mode)
// ============================================================================

/// Aggregate checkpoints across multiple workspaces.
///
/// Iterates each workspace, calls `recall()` per workspace, merges results
/// sorted by timestamp (newest first), tags each checkpoint's summary with
/// its source project name, builds `WorkspaceSummary` entries, and applies
/// a global limit.
///
/// The `active_plan` field is always `None` for cross-project results
/// (plans are per-workspace, not cross-project).
pub fn recall_cross_project(
    workspaces: Vec<(String, PathBuf)>,
    options: RecallOptions,
) -> Result<RecallResult> {
    let global_limit = options.limit.unwrap_or(DEFAULT_LIMIT);
    let full = options.full.unwrap_or(false);

    let mut all_checkpoints: Vec<Checkpoint> = Vec::new();
    let mut workspace_summaries: Vec<super::WorkspaceSummary> = Vec::new();

    for (project_name, workspace_root) in &workspaces {
        // Build per-workspace options: no limit (we apply global limit after merge),
        // keep full=true so we preserve git context for now (strip at the end).
        let per_ws_options = RecallOptions {
            workspace: None,
            since: options.since.clone(),
            days: options.days,
            from: options.from.clone(),
            to: options.to.clone(),
            search: options.search.clone(),
            limit: None, // no per-workspace limit
            full: Some(true), // keep git context for now
            plan_id: options.plan_id.clone(),
        };

        let ws_result = match recall(workspace_root, per_ws_options) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(
                    "Cross-project recall: failed to recall from '{}' at {}: {}",
                    project_name,
                    workspace_root.display(),
                    e
                );
                // Build an empty summary for this workspace and continue
                workspace_summaries.push(super::WorkspaceSummary {
                    name: project_name.clone(),
                    path: workspace_root.to_string_lossy().to_string(),
                    checkpoint_count: 0,
                    last_activity: None,
                });
                continue;
            }
        };

        // Build workspace summary
        let checkpoint_count = ws_result.checkpoints.len();
        let last_activity = ws_result
            .checkpoints
            .first()
            .map(|cp| cp.timestamp.clone());

        workspace_summaries.push(super::WorkspaceSummary {
            name: project_name.clone(),
            path: workspace_root.to_string_lossy().to_string(),
            checkpoint_count,
            last_activity,
        });

        // Tag each checkpoint with the source project name
        let mut tagged = ws_result.checkpoints;
        for cp in &mut tagged {
            let original = cp.summary.take().unwrap_or_default();
            cp.summary = Some(format!("[{}] {}", project_name, original));
        }

        all_checkpoints.extend(tagged);
    }

    // Sort all checkpoints by timestamp descending (newest first)
    all_checkpoints.sort_unstable_by(|a, b| b.timestamp.cmp(&a.timestamp));

    // Apply global limit
    all_checkpoints.truncate(global_limit);

    // Strip git context if !full
    if !full {
        for cp in &mut all_checkpoints {
            cp.git = None;
        }
    }

    Ok(RecallResult {
        checkpoints: all_checkpoints,
        active_plan: None,
        workspaces: Some(workspace_summaries),
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
