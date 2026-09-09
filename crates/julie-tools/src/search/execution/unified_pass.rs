use anyhow::Result;
use julie_context::ToolContext;
use julie_index::search::SearchFilter;

use super::types::{SearchExecutionWorkspace, UnifiedPassResult, sort_hits_by_score_desc};
use crate::search::text_search;
use crate::search::trace::SearchHit;

/// Inner helper: run the unified Tantivy/SQLite search across all workspaces
/// with a single `file_pattern` value, apply test-exclude filtering, and
/// return both the hits and the pre-filter counts the caller needs to
/// attribute zero-hit runs.  Splitting this out lets `execute_search_unified`
/// call the same pipeline twice (scoped + unscoped) for scope rescue.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_unified_pass(
    query: &str,
    language: &Option<String>,
    file_pattern: Option<&str>,
    limit: u32,
    effective_exclude_tests: bool,
    workspaces: &[SearchExecutionWorkspace],
    handler: &dyn ToolContext,
) -> Result<UnifiedPassResult> {
    let mut hits = Vec::new();
    let mut relaxed = false;
    let mut total_results = 0usize;
    let mut pre_test_filter_total = 0usize;
    let mut pre_file_pattern_filter_total = 0usize;

    for workspace in workspaces {
        let filter = SearchFilter {
            language: language.clone(),
            kind: None,
            file_pattern: file_pattern.map(str::to_string),
            exclude_tests: effective_exclude_tests,
        };

        // Use `unified_search_hits` (returns raw UnifiedHit) rather than
        // `unified_search_impl` (converts to Symbol) so the "file" kind is
        // preserved end-to-end in the SearchHit.
        //
        // Apply language/file/test filters in the candidate source. The index
        // applies these after Tantivy materialization, so scoped zero-hit cases
        // get one wider scoped retry before any unscoped rescue is considered.
        let raw_fetch_limit = limit.saturating_mul(4).max(50);

        let (mut raw_hits, mut workspace_relaxed, mut workspace_total) =
            text_search::unified_search_hits(
                query,
                &filter,
                raw_fetch_limit,
                Some(vec![workspace.workspace_id.clone()]),
                handler,
            )
            .await?;

        if raw_hits.is_empty() && file_pattern.is_some() {
            let retry_fetch_limit = raw_fetch_limit.saturating_mul(20).max(1_000);
            if retry_fetch_limit > raw_fetch_limit {
                let (retry_hits, retry_relaxed, retry_total) = text_search::unified_search_hits(
                    query,
                    &filter,
                    retry_fetch_limit,
                    Some(vec![workspace.workspace_id.clone()]),
                    handler,
                )
                .await?;
                raw_hits = retry_hits;
                workspace_relaxed |= retry_relaxed;
                workspace_total = retry_total;
            }
        }

        relaxed |= workspace_relaxed;
        total_results += workspace_total;

        for raw_hit in raw_hits {
            hits.push(SearchHit::from_unified_hit(
                raw_hit,
                workspace.workspace_id.clone(),
            ));
        }

        if hits.is_empty() && (file_pattern.is_some() || effective_exclude_tests) {
            let diagnostic_filter = SearchFilter {
                language: language.clone(),
                kind: None,
                file_pattern: None,
                exclude_tests: false,
            };
            let diagnostic_fetch_limit = raw_fetch_limit.saturating_mul(20).max(1_000);
            let (diagnostic_hits, diagnostic_relaxed, _) = text_search::unified_search_hits(
                query,
                &diagnostic_filter,
                diagnostic_fetch_limit,
                Some(vec![workspace.workspace_id.clone()]),
                handler,
            )
            .await?;
            relaxed |= diagnostic_relaxed;
            pre_file_pattern_filter_total += diagnostic_hits.len();

            for raw_hit in diagnostic_hits {
                if let Some(pattern) = file_pattern
                    && !crate::search::matches_glob_pattern(&raw_hit.file_path, pattern)
                {
                    continue;
                }
                pre_test_filter_total += 1;
                if effective_exclude_tests
                    && (julie_index::search::scoring::is_test_path(&raw_hit.file_path)
                        || raw_hit.role == "test")
                {
                    continue;
                }
            }
        }
    }

    sort_hits_by_score_desc(&mut hits);
    hits.truncate(limit.max(1) as usize);

    Ok(UnifiedPassResult {
        hits,
        relaxed,
        total_results,
        pre_file_pattern_filter_total,
        pre_test_filter_total,
    })
}
