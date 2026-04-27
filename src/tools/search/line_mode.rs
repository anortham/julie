//! Line-mode (grep-style) search
//!
//! Provides grep-style line-by-line searching with file paths and line numbers.
//! Used when output format is set to "lines".

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::search::SearchFilter;
use crate::search::index::SearchIndex;
use crate::tools::navigation::resolution::WorkspaceTarget;

use super::hint_formatter::build_scope_rescue_header;
use super::query::{
    line_match_strategy, line_matches, looks_like_whitespace_separated_globs, matches_glob_pattern,
};
use super::trace::{FilePatternDiagnostic, ZeroHitReason};
use super::types::{LineMatch, LineMatchStrategy};

pub(crate) struct LineModeSearchResult {
    pub matches: Vec<LineMatch>,
    pub relaxed: bool,
    pub strategy: LineMatchStrategy,
    pub workspace_label: String,
    #[cfg_attr(not(test), allow(dead_code))]
    pub stage_counts: LineModeStageCounts,
    /// Post-hoc attribution of a zero-result path to a specific pipeline
    /// stage. `Some(_)` only when `matches.is_empty()`; the chosen variant
    /// is the top-most stage (Tantivy → file_pattern → language → test →
    /// content-available → line-match) whose counter drained the surviving
    /// candidate set to zero. Computed by
    /// [`attribute_zero_hit_reason`] from the raw counters in
    /// [`stage_counts`].
    pub zero_hit_reason: Option<ZeroHitReason>,
    pub file_pattern_diagnostic: Option<FilePatternDiagnostic>,
    pub scope_relaxed: bool,
    pub original_file_pattern: Option<String>,
    pub original_zero_hit_reason: Option<ZeroHitReason>,
}

struct LineModeFetchOutcome {
    matches: Vec<LineMatch>,
    relaxed: bool,
    stage_counts: LineModeStageCounts,
    file_pattern_diagnostic: Option<FilePatternDiagnostic>,
}

struct LineModeScopedOutcome {
    fetch: LineModeFetchOutcome,
    scope_relaxed: bool,
    original_file_pattern: Option<String>,
    original_zero_hit_reason: Option<ZeroHitReason>,
}

/// Per-stage drop counters for the `line_mode_matches` pipeline.
///
/// Each field records how many candidates each filter stage dropped.
/// Used for zero-hit diagnosis: when `matches.is_empty()`, the first
/// stage where `count_before > 0 && count_after == 0` is the culprit.
/// Task 4 translates this into a `ZeroHitReason` enum; Task 3 keeps
/// the raw counts so the replay report can attribute every zero-hit
/// to a specific stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LineModeStageCounts {
    /// AND-stage candidate count surfaced by `SearchIndex::search_content`.
    pub and_candidates: usize,
    /// OR-stage candidate count (0 if OR fallback was not invoked).
    pub or_candidates: usize,
    /// Files fed from Tantivy into the per-file filter loop.
    pub tantivy_file_candidates: usize,
    /// Files dropped by the `file_pattern` filter inside the loop.
    pub file_pattern_dropped: usize,
    /// Files dropped by the language filter inside the loop.
    pub language_dropped: usize,
    /// Files dropped by the `exclude_tests` filter inside the loop.
    pub test_dropped: usize,
    /// Files where `get_file_content` returned `None` (content unavailable).
    pub file_content_unavailable_dropped: usize,
    /// Files that passed every filter but produced zero line-level matches.
    pub line_match_miss_dropped: usize,
}

pub(crate) fn query_uses_file_level_header(query: &str) -> bool {
    matches!(
        line_match_strategy(query),
        LineMatchStrategy::FileLevel { .. }
    )
}

fn scoped_fetch_limits(base_limit: usize, has_file_filter: bool) -> (usize, usize) {
    if has_file_filter {
        let hard_cap = base_limit.saturating_mul(100).max(1000).min(2000);
        let initial = base_limit.saturating_mul(20).max(100).min(hard_cap);
        (initial, hard_cap)
    } else {
        let fetch_limit = base_limit.saturating_mul(20);
        (fetch_limit, fetch_limit)
    }
}

fn widened_probe_limit(initial_fetch_limit: usize) -> usize {
    initial_fetch_limit
        .saturating_mul(2)
        .max(initial_fetch_limit.saturating_add(1))
        .min(2000)
}

fn diagnose_scoped_file_pattern_miss<S>(
    search_once: &mut S,
    file_pattern: Option<&str>,
    file_results: &[crate::search::index::ContentSearchResult],
    fetch_limit: usize,
    hard_cap: usize,
) -> Result<Option<FilePatternDiagnostic>>
where
    S: FnMut(usize) -> Result<crate::search::index::ContentSearchResults>,
{
    let Some(pattern) = file_pattern else {
        return Ok(None);
    };
    if file_results.is_empty() {
        return Ok(None);
    }
    if file_results
        .iter()
        .any(|result| matches_glob_pattern(&result.file_path, pattern))
    {
        return Ok(None);
    }
    if file_results.len() < fetch_limit {
        return Ok(Some(FilePatternDiagnostic::NoInScopeCandidates));
    }

    let probe_limit = widened_probe_limit(fetch_limit).min(hard_cap);
    if probe_limit <= fetch_limit {
        return Ok(Some(FilePatternDiagnostic::NoInScopeCandidates));
    }

    let wider_results = search_once(probe_limit)?;
    if wider_results
        .results
        .iter()
        .any(|result| matches_glob_pattern(&result.file_path, pattern))
    {
        Ok(Some(FilePatternDiagnostic::CandidateStarvation))
    } else {
        Ok(Some(FilePatternDiagnostic::NoInScopeCandidates))
    }
}

fn collect_matches_from_file_results(
    db: &crate::database::SymbolDatabase,
    file_results: &[crate::search::index::ContentSearchResult],
    file_pattern: Option<&str>,
    language: Option<&str>,
    exclude_test_files: bool,
    match_strategy: &LineMatchStrategy,
    base_limit: usize,
) -> Result<(Vec<LineMatch>, LineModeStageCounts, bool)> {
    let mut counts = LineModeStageCounts::default();
    let mut matches = Vec::new();
    let mut saw_in_scope_candidate = false;

    for file_result in file_results {
        if matches.len() >= base_limit {
            break;
        }

        if let Some(pattern) = file_pattern
            && !matches_glob_pattern(&file_result.file_path, pattern)
        {
            counts.file_pattern_dropped += 1;
            continue;
        }

        if file_pattern.is_some() {
            saw_in_scope_candidate = true;
        }

        if let Some(lang) = language
            && !file_matches_language(&file_result.file_path, lang)
        {
            counts.language_dropped += 1;
            continue;
        }

        if exclude_test_files && crate::search::scoring::is_test_path(&file_result.file_path) {
            counts.test_dropped += 1;
            continue;
        }

        match db.get_file_content(&file_result.file_path)? {
            Some(content) => {
                let before = matches.len();
                collect_line_matches(
                    &mut matches,
                    &content,
                    &file_result.file_path,
                    match_strategy,
                    base_limit,
                );
                if matches.len() == before {
                    counts.line_match_miss_dropped += 1;
                }
            }
            None => {
                counts.file_content_unavailable_dropped += 1;
            }
        }
    }

    Ok((matches, counts, saw_in_scope_candidate))
}

fn run_line_mode_fetch_loop<S, C>(
    mut search_once: S,
    mut collect_attempt: C,
    file_pattern: Option<&str>,
    base_limit: usize,
    has_file_filter: bool,
) -> Result<(
    Vec<LineMatch>,
    bool,
    LineModeStageCounts,
    Option<FilePatternDiagnostic>,
)>
where
    S: FnMut(usize) -> Result<crate::search::index::ContentSearchResults>,
    C: FnMut(
        &[crate::search::index::ContentSearchResult],
    ) -> Result<(Vec<LineMatch>, LineModeStageCounts, bool)>,
{
    let (mut fetch_limit, hard_cap) = scoped_fetch_limits(base_limit, has_file_filter);

    loop {
        let content_results = search_once(fetch_limit)?;
        let relaxed = content_results.relaxed;
        let file_results = content_results.results;
        let mut counts = LineModeStageCounts {
            and_candidates: content_results.and_candidate_count,
            or_candidates: content_results.or_candidate_count,
            tantivy_file_candidates: file_results.len(),
            ..Default::default()
        };
        let (matches, collected_counts, saw_in_scope_candidate) = collect_attempt(&file_results)?;
        counts.file_pattern_dropped = collected_counts.file_pattern_dropped;
        counts.language_dropped = collected_counts.language_dropped;
        counts.test_dropped = collected_counts.test_dropped;
        counts.file_content_unavailable_dropped = collected_counts.file_content_unavailable_dropped;
        counts.line_match_miss_dropped = collected_counts.line_match_miss_dropped;

        let zero_hit_reason = if matches.is_empty() {
            attribute_zero_hit_reason(&counts)
        } else {
            None
        };
        let saturated_window = file_results.len() >= fetch_limit;
        let should_widen = has_file_filter
            && matches.is_empty()
            && zero_hit_reason == Some(ZeroHitReason::FilePatternFiltered)
            && !saw_in_scope_candidate
            && saturated_window;

        if should_widen {
            let widened_limit = widened_probe_limit(fetch_limit).min(hard_cap);
            if widened_limit > fetch_limit {
                fetch_limit = widened_limit;
                continue;
            }
        }

        let file_pattern_diagnostic =
            if matches.is_empty() && zero_hit_reason == Some(ZeroHitReason::FilePatternFiltered) {
                diagnose_scoped_file_pattern_miss(
                    &mut search_once,
                    file_pattern,
                    &file_results,
                    fetch_limit,
                    hard_cap,
                )?
            } else {
                None
            };

        return Ok((matches, relaxed, counts, file_pattern_diagnostic));
    }
}

fn run_line_mode_workspace_fetch(
    db: Arc<Mutex<SymbolDatabase>>,
    search_index: Arc<Mutex<SearchIndex>>,
    query: String,
    match_strategy: LineMatchStrategy,
    file_pattern: Option<String>,
    language: Option<String>,
    exclude_test_files: bool,
    base_limit: usize,
) -> Result<LineModeFetchOutcome> {
    let has_file_filter = file_pattern.is_some();
    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.clone(),
        exclude_tests: false,
    };

    let (matches, relaxed, stage_counts, file_pattern_diagnostic) = run_line_mode_fetch_loop(
        |fetch_limit| {
            let index = match search_index.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Search index mutex poisoned, recovering: {}", poisoned);
                    poisoned.into_inner()
                }
            };
            Ok(index.search_content(&query, &filter, fetch_limit)?)
        },
        |file_results| {
            let db_lock = match db.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Database mutex poisoned, recovering: {}", poisoned);
                    poisoned.into_inner()
                }
            };
            collect_matches_from_file_results(
                &db_lock,
                file_results,
                file_pattern.as_deref(),
                language.as_deref(),
                exclude_test_files,
                &match_strategy,
                base_limit,
            )
        },
        file_pattern.as_deref(),
        base_limit,
        has_file_filter,
    )?;

    Ok(LineModeFetchOutcome {
        matches,
        relaxed,
        stage_counts,
        file_pattern_diagnostic,
    })
}

fn run_line_mode_with_scope_rescue(
    db: Arc<Mutex<SymbolDatabase>>,
    search_index: Arc<Mutex<SearchIndex>>,
    query: String,
    match_strategy: LineMatchStrategy,
    file_pattern: Option<String>,
    language: Option<String>,
    exclude_test_files: bool,
    base_limit: usize,
) -> Result<LineModeScopedOutcome> {
    let first = run_line_mode_workspace_fetch(
        Arc::clone(&db),
        Arc::clone(&search_index),
        query.clone(),
        match_strategy.clone(),
        file_pattern.clone(),
        language.clone(),
        exclude_test_files,
        base_limit,
    )?;

    let zero_hit_reason = if first.matches.is_empty() {
        attribute_zero_hit_reason(&first.stage_counts)
    } else {
        None
    };
    let should_rescue = zero_hit_reason == Some(ZeroHitReason::FilePatternFiltered)
        && first.file_pattern_diagnostic == Some(FilePatternDiagnostic::NoInScopeCandidates)
        && file_pattern
            .as_deref()
            .is_some_and(|pattern| !looks_like_whitespace_separated_globs(pattern));

    if should_rescue {
        let fallback = run_line_mode_workspace_fetch(
            db,
            search_index,
            query,
            match_strategy,
            None,
            language,
            exclude_test_files,
            base_limit,
        )?;

        if !fallback.matches.is_empty() {
            return Ok(LineModeScopedOutcome {
                fetch: fallback,
                scope_relaxed: true,
                original_file_pattern: file_pattern,
                original_zero_hit_reason: zero_hit_reason,
            });
        }
    }

    Ok(LineModeScopedOutcome {
        fetch: first,
        scope_relaxed: false,
        original_file_pattern: None,
        original_zero_hit_reason: None,
    })
}

/// Attribute a zero-result `line_mode_matches` run to the first pipeline
/// stage that drained the surviving candidate set to zero.
///
/// The walk is top-down: Tantivy → file_pattern → language → test →
/// content-available → line-match. The first stage where
/// `count_before > 0 && count_after == 0` wins. Returns `None` if the
/// pipeline never had any candidates at all AND no single filter stage
/// drained it (which happens only when the query tokenises to zero
/// terms: `search_content` early-returns, `tantivy_file_candidates == 0`
/// and none of the drop counters fired).
///
/// The returned variant is also `None` when `matches` is non-empty —
/// that invariant is enforced by the caller, not here.
pub(crate) fn attribute_zero_hit_reason(counts: &LineModeStageCounts) -> Option<ZeroHitReason> {
    // Tantivy surfaced nothing: either the query has no usable terms, or
    // the AND+OR branches both missed. Either way, nothing entered the
    // per-file loop.
    if counts.tantivy_file_candidates == 0 {
        return Some(ZeroHitReason::TantivyNoCandidates);
    }

    // Running count of "what's still alive after each stage".
    let mut surviving = counts.tantivy_file_candidates;

    // Stage 1: file_pattern.
    let before_file_pattern = surviving;
    surviving = surviving.saturating_sub(counts.file_pattern_dropped);
    if before_file_pattern > 0 && surviving == 0 {
        return Some(ZeroHitReason::FilePatternFiltered);
    }

    // Stage 2: language (per-file check; redundant with Tantivy's own
    // SearchFilter.language in the current pipeline, so this rarely fires).
    let before_language = surviving;
    surviving = surviving.saturating_sub(counts.language_dropped);
    if before_language > 0 && surviving == 0 {
        return Some(ZeroHitReason::LanguageFiltered);
    }

    // Stage 3: exclude_tests.
    let before_test = surviving;
    surviving = surviving.saturating_sub(counts.test_dropped);
    if before_test > 0 && surviving == 0 {
        return Some(ZeroHitReason::TestFiltered);
    }

    // Stage 4: file content unavailable (blob missing / storage error).
    let before_content = surviving;
    surviving = surviving.saturating_sub(counts.file_content_unavailable_dropped);
    if before_content > 0 && surviving == 0 {
        return Some(ZeroHitReason::FileContentUnavailable);
    }

    // Stage 5: line-match miss. Every remaining file had content but
    // produced zero line hits.
    let before_line = surviving;
    surviving = surviving.saturating_sub(counts.line_match_miss_dropped);
    if before_line > 0 && surviving == 0 {
        return Some(ZeroHitReason::LineMatchMiss);
    }

    // Pipeline had candidates that survived every drop counter but still
    // `matches.is_empty()` at the top level. That can happen when the
    // caller's `limit` was 0 (no files walked) or a future stage is added
    // without updating this attribution. Falling back to LineMatchMiss
    // keeps telemetry consistent (something took out the candidates;
    // attribute to the closest thing we instrument) rather than leaving
    // the reason unattributed.
    if counts.tantivy_file_candidates > 0 {
        Some(ZeroHitReason::LineMatchMiss)
    } else {
        None
    }
}

/// Line-level search mode (grep-style output with line numbers)
///
/// Returns every line matching the query with file:line_number:line_content format.
/// Supports language and file pattern filtering for targeted searches.
///
/// Accepts a pre-resolved `WorkspaceTarget` to avoid redundant workspace resolution.
/// The caller (`FastSearchTool::call_tool`) resolves the workspace once and passes it here.
#[allow(dead_code)]
pub async fn line_mode_search(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    exclude_tests: Option<bool>,
    workspace_target: &WorkspaceTarget,
    handler: &JulieServerHandler,
) -> Result<CallToolResult> {
    let result = line_mode_matches(
        query,
        language,
        file_pattern,
        limit,
        exclude_tests,
        workspace_target,
        handler,
    )
    .await?;

    if result.matches.is_empty() {
        let message = format!(
            "🔍 No lines found matching: '{}'\n\
            💡 Try search_target=\"definitions\" if looking for a symbol name, or broaden file_pattern/language filters",
            query
        );
        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
    }

    Ok(CallToolResult::text_content(vec![Content::text(
        format_line_mode_output(query, &result),
    )]))
}

pub(crate) async fn line_mode_matches(
    query: &str,
    language: &Option<String>,
    file_pattern: &Option<String>,
    limit: u32,
    exclude_tests: Option<bool>,
    workspace_target: &WorkspaceTarget,
    handler: &JulieServerHandler,
) -> Result<LineModeSearchResult> {
    debug!("📄 Line-level search for: '{}'", query);

    let exclude_test_files = exclude_tests.unwrap_or(false);
    let workspace_label = match workspace_target {
        WorkspaceTarget::Primary => "primary".to_string(),
        WorkspaceTarget::Target(id) => id.clone(),
    };
    let match_strategy = line_match_strategy(query);
    let base_limit = limit.max(1) as usize;

    let scoped_outcome = match workspace_target {
        WorkspaceTarget::Primary => {
            let primary_snapshot = handler.primary_workspace_snapshot().await?;
            let db = primary_snapshot.database;
            let search_index = primary_snapshot.search_index.ok_or_else(|| {
                    anyhow::anyhow!(
                        "Line-level content search requires a Tantivy index for the current primary workspace. Run manage_workspace(operation=\"refresh\") first."
                    )
                })?;

            let query = query.to_string();
            let match_strategy = match_strategy.clone();
            let file_pattern_clone = file_pattern.clone();
            let language_clone = language.clone();

            tokio::task::spawn_blocking(move || {
                run_line_mode_with_scope_rescue(
                    db,
                    search_index,
                    query,
                    match_strategy,
                    file_pattern_clone,
                    language_clone,
                    exclude_test_files,
                    base_limit,
                )
            })
            .await??
        }
        WorkspaceTarget::Target(workspace_id) => {
            let db_arc = handler.get_database_for_workspace(workspace_id).await?;
            let si_arc = handler.get_search_index_for_workspace(workspace_id).await?;
            let target_workspace_id = workspace_id.clone();

            let query_clone = query.to_string();
            let strategy = match_strategy.clone();
            let ref_file_pattern = file_pattern.clone();
            let ref_language = language.clone();

            tokio::task::spawn_blocking(move || -> Result<LineModeScopedOutcome> {
                let search_index = match si_arc {
                    Some(si) => si,
                    None => {
                        return Err(anyhow::anyhow!(
                            "Line-level content search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.",
                            target_workspace_id,
                            target_workspace_id
                        ));
                    }
                };

                run_line_mode_with_scope_rescue(
                    db_arc,
                    search_index,
                    query_clone,
                    strategy,
                    ref_file_pattern,
                    ref_language,
                    exclude_test_files,
                    base_limit,
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("Failed to spawn target workspace search: {}", e))??
        }
    };
    let all_line_matches = scoped_outcome.fetch.matches;
    let relaxed = scoped_outcome.fetch.relaxed;
    let stage_counts = scoped_outcome.fetch.stage_counts;
    let scoped_file_pattern_diagnostic = scoped_outcome.fetch.file_pattern_diagnostic;

    // Task 5: the second-pass filter that used to live here re-ran the
    // caller's `file_pattern`, `language`, and `exclude_tests` checks on
    // `line_match.file_path`. That was redundant with the per-file loop
    // above — every file reaching `collect_line_matches` has already
    // passed those same three checks, and `collect_line_matches` copies
    // `file_result.file_path` verbatim into each `LineMatch`. So the
    // second pass had nothing to drop. See
    // `tests::tools::search::line_mode_second_pass_tests` for the
    // invariant; reintroduce a second pass only if the per-file loop
    // ever starts producing matches from files it didn't fully validate.
    let zero_hit_reason = if all_line_matches.is_empty() {
        attribute_zero_hit_reason(&stage_counts)
    } else {
        None
    };
    let file_pattern_diagnostic = if all_line_matches.is_empty()
        && zero_hit_reason == Some(ZeroHitReason::FilePatternFiltered)
    {
        scoped_file_pattern_diagnostic
    } else {
        None
    };

    Ok(LineModeSearchResult {
        matches: all_line_matches,
        relaxed,
        strategy: match_strategy,
        workspace_label,
        stage_counts,
        zero_hit_reason,
        file_pattern_diagnostic,
        scope_relaxed: scoped_outcome.scope_relaxed,
        original_file_pattern: scoped_outcome.original_file_pattern,
        original_zero_hit_reason: scoped_outcome.original_zero_hit_reason,
    })
}

pub(crate) fn format_line_mode_output(query: &str, result: &LineModeSearchResult) -> String {
    let header = match &result.strategy {
        LineMatchStrategy::FileLevel { .. } => {
            let file_count = result
                .matches
                .iter()
                .map(|m| &m.file_path)
                .collect::<std::collections::HashSet<_>>()
                .len();
            format!(
                "📄 File-level search in [{}]: '{}' (found {} lines across {} files)",
                result.workspace_label,
                query,
                result.matches.len(),
                file_count
            )
        }
        _ => format!(
            "📄 Line-level search in [{}]: '{}' (found {} lines)",
            result.workspace_label,
            query,
            result.matches.len()
        ),
    };
    let mut lines = Vec::new();
    if result.scope_relaxed
        && let Some(original_file_pattern) = result.original_file_pattern.as_deref()
    {
        if result.workspace_label == "multiple" {
            lines.push(format!(
                "NOTE: At least one workspace had 0 matches within file_pattern={}. Showing {} aggregated results. Scope-rescued results may be outside requested scope.",
                original_file_pattern,
                result.matches.len(),
            ));
        } else {
            lines.push(build_scope_rescue_header(
                original_file_pattern,
                result.matches.len(),
            ));
        }
        lines.push(String::new());
    }
    lines.push(header);
    lines.push(String::new());

    for entry in &result.matches {
        lines.push(format!(
            "{}:{}:{}",
            entry.file_path, entry.line_number, entry.line_content
        ));
    }

    lines.join("\n")
}

/// Check if a file path matches the given language filter by extension.
fn file_matches_language(file_path: &str, lang: &str) -> bool {
    let path = std::path::Path::new(file_path);
    let Some(ext) = path.extension() else {
        return false;
    };
    let ext_str = ext.to_string_lossy().to_lowercase();
    match lang.to_lowercase().as_str() {
        "rust" => ext_str == "rs",
        "typescript" => ext_str == "ts" || ext_str == "tsx",
        "javascript" => ext_str == "js" || ext_str == "jsx" || ext_str == "mjs",
        "python" => ext_str == "py",
        "java" => ext_str == "java",
        "csharp" | "c#" => ext_str == "cs",
        "cpp" | "c++" => ext_str == "cpp" || ext_str == "cc" || ext_str == "cxx",
        "c" => ext_str == "c" || ext_str == "h",
        _ => ext_str == lang.to_lowercase(),
    }
}

/// Collect line matches from file content using the given strategy
fn collect_line_matches(
    destination: &mut Vec<LineMatch>,
    content: &str,
    file_path: &str,
    strategy: &LineMatchStrategy,
    max_results: usize,
) {
    if destination.len() >= max_results {
        return;
    }
    for (line_idx, line) in content.lines().enumerate() {
        if line_matches(strategy, line) {
            destination.push(LineMatch {
                file_path: file_path.to_string(),
                line_number: line_idx + 1,
                line_content: line.trim_end_matches('\r').to_string(),
            });

            if destination.len() >= max_results {
                break;
            }
        }
    }
}
