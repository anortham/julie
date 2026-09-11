use std::sync::Arc;

use anyhow::Result;
use julie_index::search::SearchFilter;
use julie_index::snapshot::Snapshot;

use super::types::{SearchExecutionWorkspace, UnifiedPassResult, sort_hits_by_score_desc};
use crate::search::text_search;
use crate::search::trace::SearchHit;

/// Run the unified search over the snapshot with a single `file_pattern`
/// value, apply test-exclude filtering, and return both the hits and the
/// pre-filter counts the caller needs to attribute zero-hit runs.
/// `execute_search_unified` calls this twice (scoped + unscoped) for scope
/// rescue.
pub(crate) async fn run_unified_pass(
    query: &str,
    language: &Option<String>,
    file_pattern: Option<&str>,
    limit: u32,
    effective_exclude_tests: bool,
    workspace: &SearchExecutionWorkspace,
    snapshot: &Arc<Snapshot>,
) -> Result<UnifiedPassResult> {
    let mut hits = Vec::new();
    let mut pre_test_filter_total = 0usize;
    let mut pre_file_pattern_filter_total = 0usize;

    let filter = SearchFilter {
        language: language.clone(),
        kind: None,
        file_pattern: file_pattern.map(str::to_string),
        exclude_tests: effective_exclude_tests,
    };

    // The index applies language/file/test filters after Tantivy
    // materialization, so scoped zero-hit cases get one wider scoped retry
    // before any unscoped rescue is considered.
    let raw_fetch_limit = limit
        .saturating_mul(4)
        .max(crate::search::params::MAX_LIMIT);

    let (mut raw_hits, mut relaxed, mut total_results) =
        text_search::unified_search_hits(query, &filter, raw_fetch_limit, snapshot).await?;

    if raw_hits.is_empty() && file_pattern.is_some() {
        let retry_fetch_limit = raw_fetch_limit.saturating_mul(20).max(1_000);
        if retry_fetch_limit > raw_fetch_limit {
            let (retry_hits, retry_relaxed, retry_total) =
                text_search::unified_search_hits(query, &filter, retry_fetch_limit, snapshot)
                    .await?;
            raw_hits = retry_hits;
            relaxed |= retry_relaxed;
            total_results = retry_total;
        }
    }

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
            snapshot,
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
