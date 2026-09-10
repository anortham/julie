use anyhow::Result;
use julie_context::ToolContext;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use julie_core::shared::OptimizedResponse;

use super::backend::SearchBackend;
use super::formatting;
use super::hint_formatter;
use super::line_enrichment::line_match_strategy_label;
use super::line_mode;
use super::params::{FastSearchExecution, FastSearchParams};
use super::regions;
use super::trace::{self, SearchExecutionResult, SearchHit, ZeroHitReason};
use crate::navigation::resolution::WorkspaceTarget;

impl FastSearchParams {
    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        self.execute_with_trace(handler).await.map(|run| run.result)
    }

    pub async fn execute_with_trace(
        &self,
        handler: &dyn ToolContext,
    ) -> Result<FastSearchExecution> {
        if self.regions.is_none() {
            return self.search.execute_with_trace(handler).await;
        }
        let workspace_target = self.search.resolve_workspace_filter(handler).await?;
        self.execute_with_trace_with_target(handler, workspace_target)
            .await
    }

    pub async fn execute_with_trace_with_target(
        &self,
        handler: &dyn ToolContext,
        workspace_target: WorkspaceTarget,
    ) -> Result<FastSearchExecution> {
        self.execute_with_trace_with_target_and_budget(handler, workspace_target, None)
            .await
    }

    pub async fn execute_with_trace_with_target_and_budget(
        &self,
        handler: &dyn ToolContext,
        workspace_target: WorkspaceTarget,
        budget: Option<julie_core::embeddings_contract::EmbeddingRequestBudget>,
    ) -> Result<FastSearchExecution> {
        let Some(regions) = self.regions.as_deref() else {
            return self
                .search
                .execute_with_trace_with_target_and_budget(handler, workspace_target, budget)
                .await;
        };
        if matches!(
            self.search.backend,
            Some(SearchBackend::Semantic | SearchBackend::Hybrid)
        ) {
            anyhow::bail!(
                "regions require lexical search; semantic and hybrid backends search symbols"
            );
        }

        let region_filter = regions::SourceRegionFilter::parse(regions)?;
        let snapshot = handler.snapshot(&workspace_target).await?;
        let line_result = line_mode::line_mode_matches_in_snapshot(
            &self.search.query,
            &self.search.language,
            &self.search.file_pattern,
            self.search.effective_limit(),
            self.search.exclude_tests,
            snapshot,
            Some(region_filter),
        )
        .await?;
        let workspace_label = match &workspace_target {
            WorkspaceTarget::Primary => handler
                .require_primary_workspace_identity()
                .unwrap_or_else(|_| "primary".to_string()),
            WorkspaceTarget::Target(id) => id.clone(),
        };
        let requested_language = self.search.language.clone();
        let hits = line_result
            .matches
            .into_iter()
            .map(|line_match| {
                let language = requested_language
                    .clone()
                    .or_else(|| {
                        julie_core::language::detect_language(std::path::Path::new(
                            &line_match.file_path,
                        ))
                        .map(str::to_string)
                    })
                    .unwrap_or_else(|| "text".to_string());
                SearchHit::from_line_match(line_match, workspace_label.clone(), language, 0.0)
            })
            .collect::<Vec<_>>();
        let total_results = hits.len();
        let mut execution = SearchExecutionResult::new(
            hits,
            false,
            total_results,
            "fast_search_regions",
            trace::SearchExecutionKind::Content {
                workspace_label: Some(workspace_label),
                file_level: false,
            },
        );
        execution.trace.zero_hit_reason = line_result.zero_hit_reason;
        execution.trace.line_match_strategy =
            Some(line_match_strategy_label(&line_result.strategy).to_string());
        execution.trace.file_pattern_diagnostic = line_result.file_pattern_diagnostic;
        execution.trace.scope_relaxed = line_result.scope_relaxed;
        execution.trace.original_file_pattern = line_result.original_file_pattern.clone();
        if line_result.scope_relaxed {
            execution.trace.original_zero_hit_reason = Some(ZeroHitReason::FilePatternFiltered);
            execution.trace.scope_rescue_count = 1;
        }

        let result = if execution.hits.is_empty() {
            CallToolResult::text_content(vec![Content::text(format!(
                "No results found for '{}' inside source regions: {}",
                self.search.query, regions
            ))])
        } else {
            let output = if self.search.return_format == "locations" {
                let response = OptimizedResponse::with_total(execution.hits.clone(), total_results);
                formatting::format_content_locations_only(&self.search.query, &response)
            } else {
                format_region_search_results(&self.search.query, &execution.hits)
            };
            let output = if line_result.scope_relaxed
                && let Some(pattern) = line_result.original_file_pattern.as_deref()
            {
                let distinct_files = execution
                    .hits
                    .iter()
                    .map(|hit| hit.file.as_str())
                    .collect::<std::collections::HashSet<_>>()
                    .len();
                format!(
                    "{}\n\n{}",
                    hint_formatter::build_scope_rescue_header(pattern, distinct_files),
                    output
                )
            } else {
                output
            };
            CallToolResult::text_content(vec![Content::text(output)])
        };

        Ok(FastSearchExecution {
            result,
            execution: Some(execution),
        })
    }
}

pub(crate) fn format_region_search_results(query: &str, hits: &[SearchHit]) -> String {
    let mut output = format!("{} matches for \"{}\":\n", hits.len(), query);
    for hit in hits {
        output.push_str(&format!(
            "{}:{}\n  {}\n",
            hit.file,
            hit.line.unwrap_or_default(),
            hit.snippet.as_deref().unwrap_or_default().trim()
        ));
    }
    output.trim_end().to_string()
}
