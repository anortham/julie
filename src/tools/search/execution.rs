use std::cmp::Ordering;

use anyhow::Result;

use crate::handler::JulieServerHandler;
use crate::tools::navigation::resolution::WorkspaceTarget;

use super::formatting;
use super::line_mode;
use super::text_search;
use super::trace::{
    FilePatternDiagnostic, SearchExecutionKind, SearchExecutionResult, SearchHit, ZeroHitReason,
};

pub struct SearchExecutionParams<'a> {
    pub query: &'a str,
    pub language: &'a Option<String>,
    pub file_pattern: &'a Option<String>,
    pub limit: u32,
    pub search_target: &'a str,
    pub context_lines: Option<u32>,
    pub exclude_tests: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct SearchExecutionWorkspace {
    pub workspace_id: String,
    pub target: WorkspaceTarget,
}

impl SearchExecutionWorkspace {
    pub fn primary(workspace_id: String) -> Self {
        Self {
            workspace_id,
            target: WorkspaceTarget::Primary,
        }
    }

    pub fn target(workspace_id: String) -> Self {
        Self {
            workspace_id: workspace_id.clone(),
            target: WorkspaceTarget::Target(workspace_id),
        }
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
        search_target: params.search_target,
        context_lines: params.context_lines,
        exclude_tests: params.exclude_tests,
    };

    if normalized_params.search_target == "content" {
        execute_content_search(normalized_params, workspaces, handler).await
    } else {
        execute_definition_search(normalized_params, workspaces, handler).await
    }
}

fn sort_hits_by_score_desc(hits: &mut [SearchHit]) {
    hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));
}

async fn execute_definition_search(
    params: SearchExecutionParams<'_>,
    workspaces: &[SearchExecutionWorkspace],
    handler: &JulieServerHandler,
) -> Result<SearchExecutionResult> {
    let mut hits = Vec::new();
    let mut relaxed = false;
    let mut total_results = 0usize;

    for workspace in workspaces {
        let (symbols, workspace_relaxed, workspace_total) = text_search::text_search_impl(
            params.query,
            params.language,
            params.file_pattern,
            params.limit,
            Some(vec![workspace.workspace_id.clone()]),
            "definitions",
            params.context_lines,
            params.exclude_tests,
            handler,
        )
        .await?;

        let symbols = formatting::truncate_code_context(symbols, params.context_lines);
        relaxed |= workspace_relaxed;
        total_results += workspace_total;
        hits.extend(
            symbols
                .into_iter()
                .map(|symbol| SearchHit::from_symbol(symbol, workspace.workspace_id.clone())),
        );
    }

    sort_hits_by_score_desc(&mut hits);
    hits.truncate(params.limit.max(1) as usize);

    Ok(SearchExecutionResult::new(
        hits,
        relaxed,
        total_results,
        "fast_search_definitions",
        SearchExecutionKind::Definitions,
    ))
}

async fn execute_content_search(
    params: SearchExecutionParams<'_>,
    workspaces: &[SearchExecutionWorkspace],
    handler: &JulieServerHandler,
) -> Result<SearchExecutionResult> {
    let mut hits = Vec::new();
    let mut relaxed = false;
    let mut total_results = 0usize;
    // Task 4b: capture the first non-None `zero_hit_reason` surfaced by
    // `line_mode_matches`. When the aggregated `hits` set ends up empty,
    // we copy this onto `SearchExecutionResult.trace.zero_hit_reason` so
    // MCP callers, telemetry, and the dashboard see the same pipeline
    // attribution that line_mode already computed. First-non-None wins
    // because all-zero runs share the same culprit across workspaces;
    // mixing variants would be noisier than useful.
    let mut last_zero_hit_reason: Option<ZeroHitReason> = None;
    let mut last_file_pattern_diagnostic: Option<FilePatternDiagnostic> = None;
    let file_level = line_mode::query_uses_file_level_header(params.query);
    let workspace_label = if workspaces.len() == 1 {
        match &workspaces[0].target {
            WorkspaceTarget::Primary => Some("primary".to_string()),
            WorkspaceTarget::Target(id) => Some(id.clone()),
        }
    } else {
        None
    };

    for workspace in workspaces {
        let result = line_mode::line_mode_matches(
            params.query,
            params.language,
            params.file_pattern,
            params.limit,
            params.exclude_tests,
            &workspace.target,
            handler,
        )
        .await?;

        relaxed |= result.relaxed;
        total_results += result.matches.len();
        if last_zero_hit_reason.is_none() {
            last_zero_hit_reason = result.zero_hit_reason;
        }
        if last_file_pattern_diagnostic.is_none() {
            last_file_pattern_diagnostic = result.file_pattern_diagnostic;
        }

        // Content (line-mode) hits carry a neutral 0.0 score intentionally.
        // The previous synthetic `workspace_total - idx as f32` looked like
        // a score but was count-derived ranking noise — it gave downstream
        // consumers (dashboard compare bench, telemetry, agent prompts) the
        // false impression that line-mode results had a meaningful
        // relevance signal. Real per-line BM25 is deferred (see the
        // dashboard-scoring doc); until that lands, content hits are
        // unranked and the list order is the order line_mode_matches
        // emitted them. `sort_hits_by_score_desc` becomes a stable no-op
        // here because Rust's sort preserves insertion order for equal
        // keys.
        for line_match in result.matches.into_iter() {
            hits.push(SearchHit::from_line_match(
                line_match,
                workspace.workspace_id.clone(),
                infer_language(params.language),
                0.0_f32,
            ));
        }
    }

    sort_hits_by_score_desc(&mut hits);
    hits.truncate(params.limit.max(1) as usize);

    let mut execution_result = SearchExecutionResult::new(
        hits,
        relaxed,
        total_results,
        "fast_search_content",
        SearchExecutionKind::Content {
            workspace_label,
            file_level,
        },
    );

    // Only stamp the reason when the run is genuinely zero-hit and no
    // earlier stage already set `trace.zero_hit_reason`.
    if execution_result.hits.is_empty() && execution_result.trace.zero_hit_reason.is_none() {
        execution_result.trace.zero_hit_reason = last_zero_hit_reason;
    }
    if execution_result.hits.is_empty() && execution_result.trace.file_pattern_diagnostic.is_none()
    {
        execution_result.trace.file_pattern_diagnostic = last_file_pattern_diagnostic;
    }

    Ok(execution_result)
}

fn infer_language(requested_language: &Option<String>) -> String {
    requested_language.clone().unwrap_or_default()
}
