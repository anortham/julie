use std::cmp::Ordering;

use crate::search::backend::ResolvedSearchBackend;
use crate::search::trace::SearchHit;

pub struct SearchExecutionParams<'a> {
    pub query: &'a str,
    pub language: &'a Option<String>,
    pub file_pattern: &'a Option<String>,
    pub limit: u32,
    pub context_lines: Option<u32>,
    pub exclude_tests: Option<bool>,
    pub backend: ResolvedSearchBackend,
    pub semantic_mode: Option<julie_core::embeddings_contract::SemanticMode>,
    pub budget: Option<julie_core::embeddings_contract::EmbeddingRequestBudget>,
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

/// Inner helper result: run the unified Tantivy/SQLite search across all workspaces
/// with a single `file_pattern` value, apply test-exclude filtering, and
/// return both the hits and the pre-filter counts the caller needs to
/// attribute zero-hit runs.
pub(crate) struct UnifiedPassResult {
    pub hits: Vec<SearchHit>,
    pub relaxed: bool,
    pub total_results: usize,
    /// Total raw-hit count across all workspaces *before* the file_pattern
    /// filter. Used to attribute FilePatternFiltered when this is non-zero
    /// but `pre_test_filter_total` is zero.
    pub pre_file_pattern_filter_total: usize,
    /// Total candidates that *survived* the file_pattern filter and entered
    /// the test-exclude stage. Used to attribute TestFiltered when this is
    /// non-zero but the final hits vector is empty.
    pub pre_test_filter_total: usize,
}

pub(crate) fn sort_hits_by_score_desc(hits: &mut [SearchHit]) {
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
}
