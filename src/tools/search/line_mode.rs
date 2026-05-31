//! Line-mode (grep-style) search
//!
//! Provides grep-style line-by-line searching with file paths and line numbers.
//! Used when output format is set to "lines".

use anyhow::Result;
use std::sync::{Arc, Mutex};
use tracing::{debug, warn};

use crate::database::SymbolDatabase;
use crate::handler::JulieServerHandler;
use crate::search::SearchFilter;
use crate::search::index::SearchIndex;
use crate::search::query_parse::{QueryIntent, parse_query};
use crate::search::scoring::{is_nl_like_query, is_test_path};
use crate::tools::navigation::resolution::WorkspaceTarget;

use super::query::{
    line_match_strategy, line_matches, looks_like_whitespace_separated_globs, matches_glob_pattern,
    term_matches_line, tokenize_text_for_line_match,
};
use super::trace::{FilePatternDiagnostic, ZeroHitReason};
use super::types::{LineMatch, LineMatchStrategy};

pub(crate) struct LineModeSearchResult {
    pub matches: Vec<LineMatch>,
    pub strategy: LineMatchStrategy,
    #[cfg_attr(not(test), allow(dead_code))]
    pub stage_counts: LineModeStageCounts,
    pub zero_hit_reason: Option<ZeroHitReason>,
    pub file_pattern_diagnostic: Option<FilePatternDiagnostic>,
    pub scope_relaxed: bool,
    pub original_file_pattern: Option<String>,
}

struct LineModeFetchOutcome {
    matches: Vec<LineMatch>,
    stage_counts: LineModeStageCounts,
    file_pattern_diagnostic: Option<FilePatternDiagnostic>,
}

struct LineModeCandidateWindow {
    file_results: Vec<crate::search::index::UnifiedHit>,
    saturated: bool,
}

struct LineModeScopedOutcome {
    fetch: LineModeFetchOutcome,
    scope_relaxed: bool,
    original_file_pattern: Option<String>,
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
    /// AND-stage file candidate count surfaced by the unified-search fetch.
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

pub(crate) fn effective_content_exclude_tests(
    query: &str,
    file_pattern: &Option<String>,
    exclude_tests: Option<bool>,
) -> bool {
    if let Some(explicit) = exclude_tests {
        return explicit;
    }

    if file_pattern
        .as_deref()
        .is_some_and(|pattern| is_test_path(pattern))
    {
        return false;
    }

    // T12 follow-up: queries that themselves name tests are clearly looking
    // for test artifacts ("src tests atomic operations.test.ts", "hub routes
    // test artifacts.py").  NL-default-exclude-tests would otherwise drop the
    // user's actual target.  Mirror the existing file_pattern test-path
    // override for the query string.
    if query_names_a_test(query) {
        return false;
    }

    parse_query(query).intent != QueryIntent::Test && is_nl_like_query(query)
}

/// True when the query string itself indicates the caller is looking for test
/// artifacts — either by naming a test path segment (`tests`, `__tests__`,
/// `spec`) or by carrying a test-file extension token (`.test.ts`,
/// `_test.go`, `test_*.py`, `.spec.ts`, ...).  Used by
/// [`effective_content_exclude_tests`] to skip NL-default-exclude-tests when
/// the query is itself test-shaped.
fn query_names_a_test(query: &str) -> bool {
    let q = query.to_ascii_lowercase();

    for term in q.split_whitespace() {
        match term {
            "test" | "tests" | "__tests__" | "spec" => return true,
            _ => {}
        }
    }

    let endings: &[&str] = &[
        ".test.ts",
        ".test.tsx",
        ".test.js",
        ".test.jsx",
        ".spec.ts",
        ".spec.tsx",
        ".spec.js",
        ".spec.jsx",
        "_test.go",
        "_test.c",
        "_test.cc",
        "_test.cpp",
    ];
    let go_style_test_prefixes: &[&str] = &[
        // Eros tokenises `active_help_test.go` as
        // `active help test.go`, so the last token starts with `test.`
        // followed by the original extension.  Matching any
        // `test.<short-ext>` here catches the Go (`_test.go`) and
        // C/C++ (`_test.c`, `_test.cc`, `_test.cpp`) families when
        // the leading underscore was split off into a separate token.
        "test.go", "test.c", "test.cc", "test.cpp",
    ];
    for term in q.split_whitespace() {
        for ending in endings {
            if term.ends_with(ending) {
                return true;
            }
        }
        for prefix in go_style_test_prefixes {
            if term == *prefix {
                return true;
            }
        }
        // Python `test_*.py` (file *starts* with `test_`, ends with `.py`).
        if term.starts_with("test_") && term.ends_with(".py") {
            return true;
        }
    }
    false
}

fn scoped_fetch_limits(base_limit: usize, has_file_filter: bool) -> (usize, usize) {
    if has_file_filter {
        // Line mode fetches file rows only, but scoped filters may still need
        // to walk past many high-scoring out-of-scope files before finding an
        // in-scope candidate.
        let hard_cap = 2000;
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
    file_results: &[crate::search::index::UnifiedHit],
    fetch_limit: usize,
    hard_cap: usize,
    candidate_window_saturated: bool,
) -> Result<Option<FilePatternDiagnostic>>
where
    S: FnMut(usize) -> Result<LineModeCandidateWindow>,
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
    if !candidate_window_saturated {
        return Ok(Some(FilePatternDiagnostic::NoInScopeCandidates));
    }

    let probe_limit = widened_probe_limit(fetch_limit).min(hard_cap);
    if probe_limit <= fetch_limit {
        return Ok(Some(FilePatternDiagnostic::NoInScopeCandidates));
    }

    let wider_results = search_once(probe_limit)?.file_results;
    if wider_results
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
    file_results: &[crate::search::index::UnifiedHit],
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
            && !indexed_language_matches(&file_result.language, lang)
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
    LineModeStageCounts,
    Option<FilePatternDiagnostic>,
)>
where
    S: FnMut(usize) -> Result<LineModeCandidateWindow>,
    C: FnMut(
        &[crate::search::index::UnifiedHit],
    ) -> Result<(Vec<LineMatch>, LineModeStageCounts, bool)>,
{
    let (mut fetch_limit, hard_cap) = scoped_fetch_limits(base_limit, has_file_filter);

    loop {
        let window = search_once(fetch_limit)?;
        let file_results = window.file_results;
        let mut counts = LineModeStageCounts {
            and_candidates: 0,
            or_candidates: 0,
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
        let saturated_window = window.saturated;
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
                    saturated_window,
                )?
            } else {
                None
            };

        return Ok((matches, counts, file_pattern_diagnostic));
    }
}

fn run_line_mode_workspace_fetch(
    db: &SymbolDatabase,
    search_index: &Arc<Mutex<SearchIndex>>,
    query: String,
    match_strategy: LineMatchStrategy,
    file_pattern: Option<String>,
    language: Option<String>,
    exclude_test_files: bool,
    base_limit: usize,
) -> Result<LineModeFetchOutcome> {
    let has_file_filter = file_pattern.is_some();
    // The file_pattern filter is applied externally below in
    // `collect_matches_from_file_results` so `file_pattern_dropped` stage
    // counters reflect candidates that Tantivy returned but the line-mode
    // filter rejected.  Passing it through `SearchFilter` would have
    // `search_unified` drop those candidates before the counter ever saw
    // them, breaking the stage-count contract.
    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: None,
        exclude_tests: false,
    };

    let (matches, stage_counts, file_pattern_diagnostic) = run_line_mode_fetch_loop(
        |fetch_limit| {
            let index = match search_index.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    warn!("Search index mutex poisoned, recovering: {}", poisoned);
                    poisoned.into_inner()
                }
            };
            // Ask Tantivy for file rows only. Filtering a mixed unified result
            // after the fact lets symbol-heavy workspaces starve line mode of
            // file candidates before scoped widening can do its job.
            let (file_hits, _relaxed) =
                index.search_unified_kind_filtered(&query, &filter, fetch_limit, true)?;
            let saturated = file_hits.len() >= fetch_limit;
            Ok(LineModeCandidateWindow {
                file_results: file_hits,
                saturated,
            })
        },
        |file_results| {
            collect_matches_from_file_results(
                db,
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
        stage_counts,
        file_pattern_diagnostic,
    })
}

fn run_line_mode_with_scope_rescue(
    db: &SymbolDatabase,
    search_index: &Arc<Mutex<SearchIndex>>,
    query: String,
    match_strategy: LineMatchStrategy,
    file_pattern: Option<String>,
    language: Option<String>,
    exclude_test_files: bool,
    base_limit: usize,
) -> Result<LineModeScopedOutcome> {
    let first = run_line_mode_workspace_fetch(
        db,
        search_index,
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
            });
        }
    }

    Ok(LineModeScopedOutcome {
        fetch: first,
        scope_relaxed: false,
        original_file_pattern: None,
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
/// terms: unified search early-returns, `tantivy_file_candidates == 0`
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

    let exclude_test_files = effective_content_exclude_tests(query, file_pattern, exclude_tests);
    let match_strategy = line_match_strategy(query);
    let base_limit = limit.max(1) as usize;

    let scoped_outcome = match workspace_target {
        WorkspaceTarget::Primary => {
            // Pooled DB: read-only, no mutation gate required.
            let pooled_db = handler.primary_pooled_database().await?;
            // search_index still lives behind Arc<Mutex<>>; not pool-migrated.
            let primary_snapshot = handler.primary_workspace_snapshot().await?;
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
                    &pooled_db,
                    &search_index,
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
            // Pooled DB: read-only, no mutation gate required.
            let pooled_db = handler
                .get_pooled_database_for_workspace(workspace_id)
                .await?;
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
                    &pooled_db,
                    &search_index,
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
    let LineModeFetchOutcome {
        matches: all_line_matches,
        stage_counts,
        file_pattern_diagnostic,
    } = scoped_outcome.fetch;
    let zero_hit_reason = if all_line_matches.is_empty() {
        attribute_zero_hit_reason(&stage_counts)
    } else {
        None
    };

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

    Ok(LineModeSearchResult {
        matches: all_line_matches,
        strategy: match_strategy,
        stage_counts,
        zero_hit_reason,
        file_pattern_diagnostic,
        scope_relaxed: scoped_outcome.scope_relaxed,
        original_file_pattern: scoped_outcome.original_file_pattern,
    })
}

fn indexed_language_matches(indexed: &str, requested: &str) -> bool {
    let indexed = indexed.to_ascii_lowercase();
    match requested.to_ascii_lowercase().as_str() {
        "c++" => indexed == "cpp",
        "c#" => indexed == "csharp",
        requested => indexed == requested,
    }
}

/// Collect line matches from file content using the given strategy
pub(crate) fn collect_line_matches(
    destination: &mut Vec<LineMatch>,
    content: &str,
    file_path: &str,
    strategy: &LineMatchStrategy,
    max_results: usize,
) {
    if destination.len() >= max_results {
        return;
    }

    match strategy {
        LineMatchStrategy::FileLevel { terms } => {
            // Density-based ranking for FileLevel: collect ALL matched lines,
            // score by count of distinct query terms matched on the line,
            // sort descending by density then ascending by line number.
            let deduped_terms: Vec<&str> = {
                let mut seen = std::collections::HashSet::new();
                terms
                    .iter()
                    .map(|t| t.as_str())
                    .filter(|t| seen.insert(t.to_lowercase()))
                    .collect()
            };
            if deduped_terms.is_empty() {
                return;
            }

            let mut scratch: Vec<(usize, usize, LineMatch)> = Vec::new();
            for (line_idx, line) in content.lines().enumerate() {
                let line_tokens = tokenize_text_for_line_match(line);
                let density = deduped_terms
                    .iter()
                    .filter(|t| term_matches_line(t, line, &line_tokens))
                    .count();
                if density > 0 {
                    scratch.push((
                        density,
                        line_idx,
                        LineMatch {
                            file_path: file_path.to_string(),
                            line_number: line_idx + 1,
                            line_content: line.trim_end_matches('\r').to_string(),
                        },
                    ));
                }
            }

            // Sort: density descending, line number ascending
            scratch.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.line_number.cmp(&b.2.line_number)));

            let budget = max_results - destination.len();
            destination.extend(scratch.into_iter().take(budget).map(|(_, _, m)| m));
        }
        _ => {
            // Substring and Tokens: preserve source order with early break
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
    }
}
