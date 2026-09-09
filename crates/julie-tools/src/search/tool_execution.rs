use anyhow::Result;
use tracing::debug;

use julie_context::ToolContext;
use julie_core::health_types::SystemStatus;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};

use super::backend::SearchBackend;
use super::execution;
use super::formatting;
use super::hint_formatter;
use super::input_diagnostics;
use super::line_enrichment;
use super::params::{FastSearchExecution, FastSearchTool, clamp_limit};
use super::trace::SearchExecutionResult;
use crate::navigation::resolution::WorkspaceTarget;

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

        let effective_limit = self.effective_limit();

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
                if let WorkspaceTarget::Primary = &workspace_target {
                    if handler.require_primary_workspace_identity().is_err() {
                        let message = "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first.";
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }

                    let primary_id = handler.require_primary_workspace_identity()?;
                    if handler
                        .get_database_for_workspace(&primary_id)
                        .await
                        .is_ok()
                        && handler
                            .get_search_index_for_workspace(&primary_id)
                            .await?
                            .is_none()
                    {
                        let message = missing_index_message(None);
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }
                }

                if let Some(ref target_workspace_id) = target_workspace_id {
                    if handler
                        .get_database_for_workspace(target_workspace_id)
                        .await
                        .is_err()
                    {
                        let message =
                            unknown_target_workspace_message(target_workspace_id.as_str());
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }
                    if handler
                        .get_search_index_for_workspace(target_workspace_id)
                        .await?
                        .is_none()
                    {
                        let message = missing_index_message(Some(target_workspace_id.as_str()));
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }
                }

                let message =
                    "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first.";
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

        let execution_workspaces = match &workspace_target {
            WorkspaceTarget::Primary => vec![execution::SearchExecutionWorkspace::primary(
                handler.require_primary_workspace_identity()?,
            )],
            WorkspaceTarget::Target(id) => {
                vec![execution::SearchExecutionWorkspace::target(id.clone())]
            }
        };

        match &workspace_target {
            WorkspaceTarget::Primary => {
                let primary_id = handler.require_primary_workspace_identity()?;
                if handler
                    .get_search_index_for_workspace(&primary_id)
                    .await?
                    .is_none()
                {
                    let message = missing_index_message(None);
                    return Ok(FastSearchExecution {
                        result: CallToolResult::text_content(vec![Content::text(message)]),
                        execution: None,
                    });
                }
            }
            WorkspaceTarget::Target(id) => {
                if handler.get_database_for_workspace(id).await.is_err() {
                    let message = unknown_target_workspace_message(id);
                    return Ok(FastSearchExecution {
                        result: CallToolResult::text_content(vec![Content::text(message)]),
                        execution: None,
                    });
                }
                if handler.get_search_index_for_workspace(id).await?.is_none() {
                    let message = missing_index_message(Some(id));
                    return Ok(FastSearchExecution {
                        result: CallToolResult::text_content(vec![Content::text(message)]),
                        execution: None,
                    });
                }
            }
        }

        if let Some(ref target_workspace_id) = target_workspace_id {
            if handler
                .get_database_for_workspace(target_workspace_id)
                .await
                .is_ok()
                && handler
                    .get_search_index_for_workspace(target_workspace_id)
                    .await?
                    .is_none()
            {
                let message = missing_index_message(Some(target_workspace_id));
                return Ok(FastSearchExecution {
                    result: CallToolResult::text_content(vec![Content::text(message)]),
                    execution: None,
                });
            }
        }

        let mut execution = execution::execute_search_unified(
            execution::SearchExecutionParams {
                query: &self.query,
                language: &self.language,
                file_pattern: &self.file_pattern,
                limit: effective_limit,
                context_lines: self.context_lines,
                exclude_tests: self.exclude_tests,
                backend: SearchBackend::resolve(self.backend),
                semantic_mode: self.semantics,
                budget,
            },
            &execution_workspaces,
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

        if self.return_format != "locations" && !has_exact_name_match && !symbol_backend_active {
            if let Err(err) = line_enrichment::try_enrich_with_line_mode_snippets(
                self,
                handler,
                &workspace_target,
                &mut execution,
            )
            .await
            {
                execution
                    .trace
                    .record_line_enrichment_failed(err.to_string());
            }
        }

        if self.return_format == "locations" {
            if line_enrichment::should_try_line_mode_locations(
                self,
                &execution,
                has_exact_name_match,
                symbol_backend_active,
            ) {
                match line_enrichment::try_line_mode_locations(
                    self,
                    handler,
                    &workspace_target,
                    &mut execution,
                )
                .await
                {
                    Ok(Some(locations_text)) => {
                        let final_text = if execution.relaxed {
                            format!(
                                "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                                locations_text
                            )
                        } else {
                            locations_text
                        };
                        let final_text = self.with_backend_fallback_note(final_text, &execution);
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(final_text)]),
                            execution: Some(execution),
                        });
                    }
                    Ok(None) => {}
                    Err(err) => execution
                        .trace
                        .record_line_enrichment_failed(err.to_string()),
                }
            }

            let mut locations_output = formatting::format_unified_locations(
                &self.query,
                &execution.hits,
                execution.total_results,
            );
            if execution.relaxed {
                locations_output = format!(
                    "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                    locations_output
                );
            }
            locations_output =
                line_enrichment::with_scope_rescue_header(locations_output, &execution);
            locations_output = self.with_backend_fallback_note(locations_output, &execution);
            return Ok(FastSearchExecution {
                result: CallToolResult::text_content(vec![Content::text(locations_output)]),
                execution: Some(execution),
            });
        }

        let lean_output = formatting::format_unified_search_results(
            &self.query,
            &execution.hits,
            execution.total_results,
        );

        let lean_output = if execution.relaxed {
            format!(
                "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                lean_output
            )
        } else {
            lean_output
        };

        let lean_output = if execution.trace.scope_relaxed
            && let Some(original_pattern) = execution.trace.original_file_pattern.as_deref()
        {
            let distinct_files: std::collections::HashSet<&str> =
                execution.hits.iter().map(|hit| hit.file.as_str()).collect();
            format!(
                "{}\n\n{}",
                hint_formatter::build_scope_rescue_header(original_pattern, distinct_files.len()),
                lean_output,
            )
        } else {
            lean_output
        };
        let lean_output = self.with_backend_fallback_note(lean_output, &execution);

        debug!(
            "✅ Returning unified search results ({} chars, {} results, relaxed: {})",
            lean_output.len(),
            execution.hits.len(),
            execution.relaxed,
        );
        Ok(FastSearchExecution {
            result: CallToolResult::text_content(vec![Content::text(lean_output)]),
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

fn missing_index_message(workspace_id: Option<&str>) -> String {
    match workspace_id {
        Some(id) => format!(
            "Search requires a Tantivy index for workspace '{id}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{id}\") first."
        ),
        None => "Search requires a Tantivy index for the current primary workspace. Run manage_workspace(operation=\"refresh\") first.".to_string(),
    }
}

fn unknown_target_workspace_message(workspace_id: &str) -> String {
    format!(
        "Workspace not indexed yet (id='{workspace_id}'). Run manage_workspace(operation=\"index\") first with the workspace path."
    )
}
