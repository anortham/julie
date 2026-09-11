use anyhow::Result;
use tracing::debug;

use julie_context::ToolContext;
use julie_core::health_types::SystemStatus;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};

use super::backend::SearchBackend;
use super::execution;
use super::execution::types::sort_hits_by_score_desc;
use super::formatting;
use super::hint_formatter;
use super::input_diagnostics;
use super::line_enrichment;
use super::params::{FastSearchExecution, FastSearchTool, MAX_LIMIT, clamp_limit};
use super::trace::SearchExecutionResult;
use crate::navigation::resolution::WorkspaceTarget;

const NOT_INDEXED_MESSAGE: &str =
    "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first.";

impl FastSearchTool {
    pub fn effective_limit(&self) -> u32 {
        clamp_limit(self.limit)
    }

    fn with_backend_fallback_note(
        &self,
        text: String,
        execution: &SearchExecutionResult,
    ) -> String {
        if execution.trace.strategy_id == "fast_search_semantic_fallback" {
            return format!(
                "NOTE: No lexical results. Showing semantic fallback candidates.\n\n{text}"
            );
        }

        if !execution.trace.backend_fallback {
            return text;
        }

        let backend = self.backend.unwrap_or(SearchBackend::Semantic).as_str();
        format!("NOTE: backend={backend} unavailable; fell back to lexical search\n\n{text}")
    }

    pub async fn call_tool(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
        self.execute_with_trace(handler).await.map(|run| run.result)
    }

    pub async fn execute_with_trace(
        &self,
        handler: &dyn ToolContext,
    ) -> Result<FastSearchExecution> {
        if let Some(diagnostic) = input_diagnostics::build_request_level_file_pattern_diagnostic(
            &self.query,
            self.file_pattern.as_deref(),
        ) {
            return Ok(diagnostic);
        }

        let workspace_target = self.resolve_workspace_filter(handler).await?;
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
        debug!("🔍 Fast search (unified): {}", self.query);

        if let Some(diagnostic) = input_diagnostics::build_request_level_file_pattern_diagnostic(
            &self.query,
            self.file_pattern.as_deref(),
        ) {
            return Ok(diagnostic);
        }

        let format = self.validated_format()?;

        if let WorkspaceTarget::Target(target_workspace_id) = &workspace_target {
            if let Some(index_error) = handler
                .ensure_target_workspace_indexed_if_pending(target_workspace_id)
                .await?
            {
                return Ok(FastSearchExecution {
                    result: index_error,
                    execution: None,
                });
            }
        }

        let target_workspace_id = match &workspace_target {
            WorkspaceTarget::Target(id) => Some(id.clone()),
            _ => None,
        };

        let readiness = handler
            .system_readiness(target_workspace_id.as_deref())
            .await?;

        match readiness {
            SystemStatus::NotReady => {
                let message = match &workspace_target {
                    WorkspaceTarget::Primary => NOT_INDEXED_MESSAGE.to_string(),
                    WorkspaceTarget::Target(id) => unknown_target_workspace_message(id),
                };
                return Ok(FastSearchExecution {
                    result: CallToolResult::text_content(vec![Content::text(message)]),
                    execution: None,
                });
            }
            SystemStatus::SqliteOnly { symbol_count } => {
                debug!("Search available ({} symbols indexed)", symbol_count);
            }
            SystemStatus::FullyReady { symbol_count } => {
                debug!("Search ready ({} symbols indexed)", symbol_count);
            }
        }

        let snapshot = match handler.snapshot(&workspace_target).await {
            Ok(snapshot) => snapshot,
            Err(err) => {
                debug!("No snapshot for {:?}: {}", workspace_target, err);
                let message = match &workspace_target {
                    WorkspaceTarget::Primary => NOT_INDEXED_MESSAGE.to_string(),
                    WorkspaceTarget::Target(id) => unknown_target_workspace_message(id),
                };
                return Ok(FastSearchExecution {
                    result: CallToolResult::text_content(vec![Content::text(message)]),
                    execution: None,
                });
            }
        };
        let workspace = match &workspace_target {
            WorkspaceTarget::Primary => execution::SearchExecutionWorkspace::primary(
                handler.require_primary_workspace_identity()?,
            ),
            WorkspaceTarget::Target(id) => execution::SearchExecutionWorkspace::target(id.clone()),
        };

        let mut execution = execution::execute_search_unified(
            execution::SearchExecutionParams {
                query: &self.query,
                language: &self.language,
                file_pattern: &self.file_pattern,
                limit: MAX_LIMIT,
                context_lines: self.context_lines,
                exclude_tests: self.exclude_tests,
                backend: SearchBackend::resolve(self.backend, &self.query),
                semantic_mode: self.semantics,
                budget,
            },
            &workspace,
            &snapshot,
            handler,
        )
        .await?;

        let query_lower = self.query.to_lowercase();
        let has_exact_name_match = execution.hits.iter().any(|hit| {
            if let Some(symbol) = hit.as_symbol() {
                formatting::is_definition_name_match(&symbol.name, &query_lower)
            } else {
                false
            }
        });
        execution.trace.definition_exact_match = has_exact_name_match;
        let symbol_backend_active = matches!(
            execution.trace.strategy_id.as_str(),
            "fast_search_semantic" | "fast_search_hybrid" | "fast_search_semantic_fallback"
        );

        if execution.hits.is_empty() {
            let message = if let Some((_hint_kind, hint_text)) =
                hint_formatter::build_content_zero_hit_hint(
                    &self.query,
                    self.file_pattern.as_deref(),
                    self.language.as_deref(),
                    self.exclude_tests,
                    execution.trace.zero_hit_reason.as_ref(),
                    execution.trace.file_pattern_diagnostic.as_ref(),
                ) {
                hint_text
            } else {
                format!(
                    "No results found for: '{}'\n\
                    Try a broader query, or add a file_pattern or language filter",
                    self.query
                )
            };
            let message = self.with_backend_fallback_note(message, &execution);
            return Ok(FastSearchExecution {
                result: CallToolResult::text_content(vec![Content::text(message)]),
                execution: Some(execution),
            });
        }

        let has_definition_exact_match = execution.hits.iter().any(|hit| {
            let Some(symbol) = hit.as_symbol() else {
                return false;
            };
            formatting::is_definition_name_match(&symbol.name, &query_lower)
                && !matches!(
                    hit.kind.as_str(),
                    "variable" | "constant" | "field" | "property" | "parameter"
                )
        });
        if format == "compact"
            && line_enrichment::should_try_line_mode_locations(
                self,
                &execution,
                has_definition_exact_match,
                symbol_backend_active,
            )
        {
            match line_enrichment::try_line_mode_locations(
                self,
                handler,
                &workspace_target,
                &snapshot,
                &mut execution,
            )
            .await
            {
                Ok(Some(_)) => {}
                Ok(None) => {}
                Err(err) => execution
                    .trace
                    .record_line_enrichment_failed(err.to_string()),
            }
        } else if !has_exact_name_match && !symbol_backend_active {
            if let Err(err) =
                line_enrichment::try_enrich_with_line_mode_snippets(self, &snapshot, &mut execution)
                    .await
            {
                execution
                    .trace
                    .record_line_enrichment_failed(err.to_string());
            }
        }

        let offset = self.offset as usize;
        let page_limit = self.effective_limit() as usize;
        sort_hits_by_score_desc(&mut execution.hits);
        execution.hits =
            formatting::collapse_covered_file_hits(std::mem::take(&mut execution.hits));
        let more = execution.hits.len() > offset + page_limit
            || execution.total_results > offset + page_limit;
        if offset > 0 || execution.hits.len() > page_limit {
            execution.hits = execution
                .hits
                .drain(..)
                .skip(offset)
                .take(page_limit)
                .collect();
        }
        execution.trace.refresh_hits(&execution.hits);
        let kept = execution.hits.len();

        let mut output = if format == "compact" {
            let backend = self.backend.map(SearchBackend::as_str).unwrap_or(
                if execution.trace.strategy_id == "fast_search_semantic" {
                    "semantic"
                } else {
                    "auto"
                },
            );
            formatting::render_compact(&self.query, backend, &execution.hits, offset, kept, more)
        } else {
            let mut lean = formatting::format_unified_search_results(
                &self.query,
                &execution.hits,
                execution.total_results,
            );
            if more {
                if !lean.ends_with('\n') {
                    lean.push('\n');
                }
                let quoted = format!("\"{}\"", self.query);
                lean.push_str(&crate::shared::next_line(
                    "fast_search",
                    &[("query", quoted.as_str())],
                    offset + kept,
                ));
            }
            lean
        };

        if execution.relaxed {
            output = format!(
                "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                output
            );
        }

        output = line_enrichment::with_scope_rescue_header(output, &execution);
        output = self.with_backend_fallback_note(output, &execution);

        debug!(
            "✅ Returning unified search results ({} chars, {} results, relaxed: {})",
            output.len(),
            execution.hits.len(),
            execution.relaxed,
        );
        Ok(FastSearchExecution {
            result: CallToolResult::text_content(vec![Content::text(output)]),
            execution: Some(execution),
        })
    }

    pub async fn resolve_workspace_filter(
        &self,
        handler: &dyn ToolContext,
    ) -> Result<WorkspaceTarget> {
        handler
            .resolve_workspace_target(self.workspace.as_deref())
            .await
    }
}

fn unknown_target_workspace_message(workspace_id: &str) -> String {
    format!(
        "Workspace not indexed yet (id='{workspace_id}'). Run manage_workspace(operation=\"index\") first with the workspace path."
    )
}
