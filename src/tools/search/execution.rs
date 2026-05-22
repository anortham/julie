use std::cmp::Ordering;

use anyhow::Result;

use crate::handler::JulieServerHandler;

use super::hint_formatter;
use super::line_mode;
use super::query;
use super::text_search;
use super::trace::{
    FilePatternDiagnostic, SearchExecutionKind, SearchExecutionResult, SearchHit, ZeroHitReason,
};

pub struct SearchExecutionParams<'a> {
    pub query: &'a str,
    pub language: &'a Option<String>,
    pub file_pattern: &'a Option<String>,
    pub limit: u32,
    pub context_lines: Option<u32>,
    pub exclude_tests: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SearchExecutionWorkspace {
    pub workspace_id: String,
}

impl SearchExecutionWorkspace {
    pub fn primary(workspace_id: String) -> Self {
        Self { workspace_id }
    }

    pub fn target(workspace_id: String) -> Self {
        Self { workspace_id }
    }
}

pub async fn execute_search(
    params: SearchExecutionParams<'_>,
    workspaces: &[SearchExecutionWorkspace],
    handler: &JulieServerHandler,
) -> Result<SearchExecutionResult> {
    // Normalize empty/whitespace-only file_pattern to None so every caller
    // (FastSearchTool, dashboard route, compare bench, …) gets the same
    // "no filter" behavior instead of an empty-pattern match-nothing. This
    // runs once at the shared entry point; downstream stages must never
    // observe a blank file_pattern.
    let normalized_file_pattern: Option<String> = params.file_pattern.as_ref().and_then(|s| {
        if s.trim().is_empty() {
            None
        } else {
            Some(s.clone())
        }
    });

    let normalized_params = SearchExecutionParams {
        query: params.query,
        language: params.language,
        file_pattern: &normalized_file_pattern,
        limit: params.limit,
        context_lines: params.context_lines,
        exclude_tests: params.exclude_tests,
    };

    // T8 cutover: all traffic routes through the unified path.
    // The per-target execute_* functions (execute_definition_search,
    // execute_content_search, execute_file_search) still exist but are
    // unreachable from production callers; T9 will delete them.
    execute_search_unified(normalized_params, workspaces, handler).await
}

fn sort_hits_by_score_desc(hits: &mut [SearchHit]) {
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
}

// ---------------------------------------------------------------------------
// Phase 2 — unified execution path
// ---------------------------------------------------------------------------

/// Execute a unified BM25 search across all FTS fields, returning mixed-kind
/// [`SearchHit`]s.  No `doc_type` filter — symbol rows and file rows both
/// contribute to the result set.
pub async fn execute_search_unified(
    params: SearchExecutionParams<'_>,
    workspaces: &[SearchExecutionWorkspace],
    handler: &JulieServerHandler,
) -> Result<SearchExecutionResult> {

    // Normalize empty/whitespace-only file_pattern to None so callers that
    // bypass `execute_search` (e.g., `FastSearchTool::execute_with_trace`)
    // get the same "no filter" behaviour as the rest of the pipeline.
    let normalized_file_pattern: Option<String> = params.file_pattern.as_ref().and_then(|s| {
        if s.trim().is_empty() {
            None
        } else {
            Some(s.clone())
        }
    });

    // T8 follow-up: apply the NL-default-exclude-tests rule that
    // `execute_content_search` used to provide.  When the caller passes
    // `exclude_tests: None`, default to excluding tests if the query looks
    // natural-language-like AND the caller did not scope to a test
    // file_pattern.  Explicit `exclude_tests: Some(_)` always wins.
    let effective_exclude_tests = line_mode::effective_content_exclude_tests(
        params.query,
        &normalized_file_pattern,
        params.exclude_tests,
    );

    // First pass: run the unified search with the caller's file_pattern.
    let first = run_unified_pass(
        params.query,
        params.language,
        normalized_file_pattern.as_deref(),
        params.limit,
        effective_exclude_tests,
        workspaces,
        handler,
    )
    .await?;

    let mut execution = SearchExecutionResult::new(
        first.hits,
        false, // unified path does not propagate per-workspace relaxed flag to callers
        first.total_results,
        "search_unified",
        SearchExecutionKind::Definitions,
    );

    // Stamp OR-disjunction detection on every run (matches the legacy
    // execute_content_search behaviour so callers and telemetry see the same
    // signal regardless of hit count).
    execution.trace.or_disjunction_detected =
        query::clean_or_disjunction_terms(params.query).is_some();

    // Zero-hit attribution (first filter wins): file_pattern drops candidates
    // before test-exclude does, so attribute to FilePatternFiltered when the
    // pattern dropped every candidate that the index produced.  Otherwise
    // attribute to TestFiltered when the exclude-tests filter ate the rest.
    let mut zero_hit_reason: Option<ZeroHitReason> = None;
    let mut file_pattern_diagnostic: Option<FilePatternDiagnostic> = None;
    if execution.hits.is_empty() {
        if normalized_file_pattern.is_some()
            && first.pre_file_pattern_filter_total > 0
            && first.pre_test_filter_total == 0
        {
            zero_hit_reason = Some(ZeroHitReason::FilePatternFiltered);
            file_pattern_diagnostic = Some(FilePatternDiagnostic::NoInScopeCandidates);
        } else if effective_exclude_tests && first.pre_test_filter_total > 0 {
            zero_hit_reason = Some(ZeroHitReason::TestFiltered);
        }
    }

    // Scope rescue: when the scoped miss is a real out-of-scope request
    // (NoInScopeCandidates) and the pattern is not a whitespace-separated
    // multi-glob mistake, re-run the unified search without the file_pattern.
    // If the unscoped run yields hits, surface them with the scope-relaxed
    // markers so callers see "0 in scope; here is what exists outside scope".
    let should_rescue = zero_hit_reason == Some(ZeroHitReason::FilePatternFiltered)
        && file_pattern_diagnostic == Some(FilePatternDiagnostic::NoInScopeCandidates)
        && normalized_file_pattern
            .as_deref()
            .is_some_and(|pattern| !query::looks_like_whitespace_separated_globs(pattern));

    if should_rescue {
        let rescue = run_unified_pass(
            params.query,
            params.language,
            None,
            params.limit,
            effective_exclude_tests,
            workspaces,
            handler,
        )
        .await?;

        if !rescue.hits.is_empty() {
            execution.hits = rescue.hits;
            execution.total_results = rescue.total_results;
            execution.trace.scope_relaxed = true;
            execution.trace.scope_rescue_count = 1;
            execution.trace.original_file_pattern = normalized_file_pattern.clone();
            execution.trace.original_zero_hit_reason = zero_hit_reason.take();
            // Clear in-scope diagnostics — the public trace now describes the
            // rescued (out-of-scope) result set rather than the scoped miss.
            file_pattern_diagnostic = None;
        }
    }

    // Persist surviving zero-hit attribution on the trace.
    execution.trace.zero_hit_reason = zero_hit_reason;
    execution.trace.file_pattern_diagnostic = file_pattern_diagnostic.clone();

    // When the run still ends with zero hits, run the content zero-hit hint
    // formatter so MCP callers receive a targeted recovery hint instead of
    // the generic "no results" message.  The hint_kind on the trace lets the
    // rendering layer pick the right text block.
    if execution.hits.is_empty()
        && let Some((hint_kind, _hint_text)) = hint_formatter::build_content_zero_hit_hint(
            params.query,
            normalized_file_pattern.as_deref(),
            params.language.as_deref(),
            params.exclude_tests,
            execution.trace.zero_hit_reason.as_ref(),
            file_pattern_diagnostic.as_ref(),
        )
    {
        execution.trace.target_hint =
            super::trace::target_hint_label(&hint_kind).map(str::to_string);
        execution.trace.hint_kind = Some(hint_kind);
    }

    Ok(execution)
}

/// Inner helper: run the unified Tantivy/SQLite search across all workspaces
/// with a single `file_pattern` value, apply test-exclude filtering, and
/// return both the hits and the pre-filter counts the caller needs to
/// attribute zero-hit runs.  Splitting this out lets `execute_search_unified`
/// call the same pipeline twice (scoped + unscoped) for scope rescue.
struct UnifiedPassResult {
    hits: Vec<SearchHit>,
    total_results: usize,
    /// Total raw-hit count across all workspaces *before* the file_pattern
    /// filter.  Used to attribute FilePatternFiltered when this is non-zero
    /// but `pre_test_filter_total` is zero.
    pre_file_pattern_filter_total: usize,
    /// Total candidates that *survived* the file_pattern filter and entered
    /// the test-exclude stage.  Used to attribute TestFiltered when this is
    /// non-zero but the final hits vector is empty.
    pre_test_filter_total: usize,
}

#[allow(clippy::too_many_arguments)]
async fn run_unified_pass(
    query: &str,
    language: &Option<String>,
    file_pattern: Option<&str>,
    limit: u32,
    effective_exclude_tests: bool,
    workspaces: &[SearchExecutionWorkspace],
    handler: &JulieServerHandler,
) -> Result<UnifiedPassResult> {
    use crate::search::SearchFilter;

    let mut hits = Vec::new();
    let mut total_results = 0usize;
    let mut pre_test_filter_total = 0usize;
    let mut pre_file_pattern_filter_total = 0usize;

    for workspace in workspaces {
        // Run the unified search WITHOUT post-filters so we can observe which
        // filter (if any) drops every candidate and attribute the zero-hit
        // run accordingly.  Tantivy's internal scoring still uses the query
        // to rank.
        let filter = SearchFilter {
            language: language.clone(),
            kind: None,
            file_pattern: None,
            exclude_tests: false,
        };

        // Use `unified_search_hits` (returns raw UnifiedHit) rather than
        // `unified_search_impl` (converts to Symbol) so the "file" kind is
        // preserved end-to-end in the SearchHit.
        //
        // Fix #1 (codex review): overfetch before applying external filters.
        // Requesting exactly `limit` raw hits and then applying file_pattern /
        // exclude_tests afterwards means valid in-scope hits ranked beyond
        // position `limit` are silently dropped.  Request a larger candidate
        // pool so the filters have headroom to yield `limit` in-scope hits.
        // The final truncation to `limit` happens after the loop below.
        let raw_fetch_limit = limit.saturating_mul(4).max(50);

        let (raw_hits, workspace_total) = text_search::unified_search_hits(
            query,
            &filter,
            raw_fetch_limit,
            Some(vec![workspace.workspace_id.clone()]),
            handler,
        )
        .await?;

        total_results += workspace_total;
        pre_file_pattern_filter_total += raw_hits.len();

        for raw_hit in raw_hits {
            // Stage 1: file_pattern filter.
            if let Some(pattern) = file_pattern
                && !crate::tools::search::matches_glob_pattern(&raw_hit.file_path, pattern)
            {
                continue;
            }
            pre_test_filter_total += 1;

            // Stage 2: NL-default-exclude-tests filter.
            //
            // Fix #2 (codex review): also exclude symbols whose role field is
            // "test", not just those whose file path is a test path.  Inline
            // #[test] functions in production-looking source files (e.g.
            // `src/lib.rs`) have role=="test" set by the projection layer from
            // extractor metadata, but is_test_path() returns false for them.
            if effective_exclude_tests
                && (crate::search::scoring::is_test_path(&raw_hit.file_path)
                    || raw_hit.role == "test")
            {
                continue;
            }
            hits.push(SearchHit::from_unified_hit(
                raw_hit,
                workspace.workspace_id.clone(),
            ));
        }
    }

    sort_hits_by_score_desc(&mut hits);
    hits.truncate(limit.max(1) as usize);

    Ok(UnifiedPassResult {
        hits,
        total_results,
        pre_file_pattern_filter_total,
        pre_test_filter_total,
    })
}
