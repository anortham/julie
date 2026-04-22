//! Fast search tool for code intelligence
//!
//! Provides Tantivy-powered code search with support for:
//! - Code-aware tokenization (CamelCase/snake_case splitting at index time)
//! - Language and file pattern filtering
//! - Line-level grep-style search
//! - Per-workspace isolation

// Public API re-exports
pub use self::query::matches_glob_pattern;
pub use self::query_preprocessor::{
    PreprocessedQuery, QueryType, detect_query_type, preprocess_query, sanitize_query,
    validate_query,
};
pub use self::types::{LineMatch, LineMatchStrategy};

// Internal modules
pub(crate) mod execution;
pub(crate) mod formatting; // Exposed for testing
pub(crate) mod hint_formatter;
pub(crate) mod line_mode;
mod nl_embeddings;
pub(crate) mod query;
pub mod query_preprocessor; // Public for testing
pub mod text_search;
pub(crate) mod trace;
mod types;

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::health::SystemStatus;
use crate::tools::navigation::resolution::WorkspaceTarget;
use crate::tools::shared::OptimizedResponse;

//******************//
//   Search Tools   //
//******************//

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
/// Search code using text search with code-aware tokenization. Supports multi-word queries with AND/OR logic. For conceptual queries (e.g., "error handling"), use search_target="definitions" to leverage semantic search.
pub struct FastSearchTool {
    /// Search query. Exact symbol names work best for definition search. Too many results? Add file_pattern or language filter. Zero results? Run manage_workspace(operation="index")
    pub query: String,
    /// Search target: "content" (default, line-level text search) or "definitions" (promotes exact symbol name matches to top, best for jumping to where a symbol is defined). Use "definitions" for conceptual queries too (e.g., "error handling retry logic") as it leverages semantic search
    #[serde(default = "default_search_target")]
    pub search_target: String,
    /// Language filter: "rust", "typescript", "javascript", "python", "java", "csharp", "vbnet", "php", "ruby", "swift", "kotlin", "scala", "go", "c", "cpp", "lua", "qml", "r", "sql", "html", "css", "vue", "bash", "gdscript", "dart", "zig"
    #[serde(default)]
    pub language: Option<String>,
    /// File pattern filter (glob syntax)
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum results (default: 10, range: 1-500)
    #[serde(
        default = "default_limit",
        deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient"
    )]
    pub limit: u32,
    /// Context lines before/after match (default: 1)
    #[serde(
        default = "default_context_lines",
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub context_lines: Option<u32>,
    /// Exclude test symbols from results.
    /// Default: auto (excludes for NL queries, includes for definition searches).
    /// Set explicitly to override.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_bool_lenient"
    )]
    pub exclude_tests: Option<bool>,
    /// Workspace filter: "primary" (default) or a workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Return format: "full" (default, code context included) or "locations" (file:line only, 70-90% fewer tokens)
    #[serde(default = "default_return_format")]
    pub return_format: String,
}

fn default_limit() -> u32 {
    10 // Reduced from 15 with enhanced scoring (better quality = fewer results needed)
}
fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}
fn default_context_lines() -> Option<u32> {
    Some(1) // 1 before + match + 1 after = 3 total lines (minimal context)
}
fn default_search_target() -> String {
    "content".to_string()
}
fn default_return_format() -> String {
    "full".to_string()
}

impl Default for FastSearchTool {
    fn default() -> Self {
        Self {
            query: String::new(),
            search_target: default_search_target(),
            language: None,
            file_pattern: None,
            limit: default_limit(),
            context_lines: default_context_lines(),
            exclude_tests: None,
            workspace: default_workspace(),
            return_format: default_return_format(),
        }
    }
}

pub(crate) struct FastSearchExecution {
    pub result: CallToolResult,
    pub execution: Option<trace::SearchExecutionResult>,
}

impl FastSearchTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        self.execute_with_trace(handler).await.map(|run| run.result)
    }

    pub(crate) async fn execute_with_trace(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<FastSearchExecution> {
        debug!(
            "🔍 Fast search: {} (target: {})",
            self.query, self.search_target
        );

        // Resolve workspace target once (used for health check and search routing)
        let workspace_target = self.resolve_workspace_filter(handler).await?;

        // Extract workspace ID for health check
        let target_workspace_id = match &workspace_target {
            WorkspaceTarget::Target(id) => Some(id.clone()),
            _ => None,
        };

        // Check system readiness
        let readiness = crate::health::HealthChecker::check_system_readiness(
            handler,
            target_workspace_id.as_deref(),
        )
        .await?;

        let use_line_mode = self.search_target != "definitions";

        match readiness {
            SystemStatus::NotReady => {
                if let WorkspaceTarget::Primary = &workspace_target {
                    if !handler.is_primary_workspace_swap_in_progress()
                        && handler.get_workspace().await?.is_none()
                    {
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
                        let message = if use_line_mode {
                            "Line-level content search requires a Tantivy index for the current primary workspace. Run manage_workspace(operation=\"refresh\") first.".to_string()
                        } else {
                            "Definition search requires a Tantivy index for the current primary workspace. Run manage_workspace(operation=\"refresh\") first.".to_string()
                        };
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
                        .is_ok()
                        && handler
                            .get_search_index_for_workspace(target_workspace_id)
                            .await?
                            .is_none()
                    {
                        let message = if use_line_mode {
                            format!(
                                "Line-level content search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.",
                                target_workspace_id, target_workspace_id
                            )
                        } else {
                            format!(
                                "Definition search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.",
                                target_workspace_id, target_workspace_id
                            )
                        };
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }
                }

                if use_line_mode {
                    debug!(
                        "Line-mode search before readiness; attempting workspace-specific resolution"
                    );
                } else {
                    let message = "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first.";
                    return Ok(FastSearchExecution {
                        result: CallToolResult::text_content(vec![Content::text(message)]),
                        execution: None,
                    });
                }
            }
            SystemStatus::SqliteOnly { symbol_count } => {
                debug!("Search available ({} symbols indexed)", symbol_count);
            }
            SystemStatus::FullyReady { symbol_count } => {
                debug!("Search ready ({} symbols indexed)", symbol_count);
            }
        }

        // Route: content search → line mode, definition search → symbol mode
        let execution_workspaces = match &workspace_target {
            WorkspaceTarget::Primary => vec![execution::SearchExecutionWorkspace::primary(
                handler.require_primary_workspace_identity()?,
            )],
            WorkspaceTarget::Target(id) => {
                vec![execution::SearchExecutionWorkspace::target(id.clone())]
            }
        };

        if use_line_mode {
            match &workspace_target {
                WorkspaceTarget::Primary => {
                    let primary_id = handler.require_primary_workspace_identity()?;
                    if handler
                        .get_search_index_for_workspace(&primary_id)
                        .await?
                        .is_none()
                    {
                        let message = "Line-level content search requires a Tantivy index for the current primary workspace. Run manage_workspace(operation=\"refresh\") first.";
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }
                }
                WorkspaceTarget::Target(id) => {
                    handler.get_database_for_workspace(id).await?;
                    if handler.get_search_index_for_workspace(id).await?.is_none() {
                        let message = format!(
                            "Line-level content search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.",
                            id, id
                        );
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }
                }
            }

            let mut execution = execution::execute_search(
                execution::SearchExecutionParams {
                    query: &self.query,
                    language: &self.language,
                    file_pattern: &self.file_pattern,
                    limit: self.limit,
                    search_target: &self.search_target,
                    context_lines: self.context_lines,
                    exclude_tests: self.exclude_tests,
                },
                &execution_workspaces,
                handler,
            )
            .await?;

            if execution.hits.is_empty() {
                // Multi-token content zero-hit → structured hint (plan §3.7).
                // Single-token zero-hits flow through the auto-promotion path
                // (Task 7); when both legs still miss, hint_kind stays None
                // and the terse fallback below fires.
                let message = if let Some(file_pattern) = self
                    .file_pattern
                    .as_deref()
                    .filter(|pattern| query::looks_like_whitespace_separated_globs(pattern))
                {
                    execution.trace.file_pattern_diagnostic =
                        Some(trace::FilePatternDiagnostic::WhitespaceSeparatedMultiGlob);
                    execution.trace.hint_kind = Some(trace::HintKind::FilePatternSyntaxHint);
                    hint_formatter::build_file_pattern_syntax_hint(&self.query, file_pattern)
                } else if hint_formatter::is_multi_token_query(&self.query) {
                    execution.trace.hint_kind = Some(trace::HintKind::MultiTokenHint);
                    hint_formatter::build_multi_token_zero_hit_hint(
                        &self.query,
                        self.file_pattern.as_deref(),
                        self.language.as_deref(),
                        self.exclude_tests,
                        execution.trace.zero_hit_reason.as_ref(),
                    )
                } else {
                    format!(
                        "🔍 No lines found matching: '{}'\n\
                        💡 Try search_target=\"definitions\" if looking for a symbol name, or broaden file_pattern/language filters",
                        self.query
                    )
                };
                return Ok(FastSearchExecution {
                    result: CallToolResult::text_content(vec![Content::text(message)]),
                    execution: Some(execution),
                });
            }

            let line_mode_result = match &execution.kind {
                trace::SearchExecutionKind::Content {
                    workspace_label,
                    file_level,
                } => line_mode::LineModeSearchResult {
                    matches: execution
                        .hits
                        .iter()
                        .filter_map(|hit| hit.as_line_match().cloned())
                        .collect(),
                    relaxed: execution.relaxed,
                    strategy: if *file_level {
                        types::LineMatchStrategy::FileLevel {
                            terms: vec![self.query.clone()],
                        }
                    } else {
                        types::LineMatchStrategy::Substring(self.query.clone())
                    },
                    workspace_label: workspace_label
                        .clone()
                        .unwrap_or_else(|| "multiple".to_string()),
                    // Stage counts are tracked inside `line_mode_matches` and
                    // consumed by the execution trace; the downstream formatter
                    // does not re-render them, so `Default` is safe here.
                    stage_counts: line_mode::LineModeStageCounts::default(),
                    // Zero-hit attribution lives on `execution.trace.zero_hit_reason`
                    // in this branch (populated via teammate-a's Task 4b wiring);
                    // the per-call `LineModeSearchResult` is only used for
                    // rendering non-empty content output here, so `None` is the
                    // honest value for the rendering-only struct.
                    zero_hit_reason: None,
                    file_pattern_diagnostic: None,
                },
                trace::SearchExecutionKind::Definitions => unreachable!("content search kind"),
            };
            let output = line_mode::format_line_mode_output(&self.query, &line_mode_result);
            return Ok(FastSearchExecution {
                result: CallToolResult::text_content(vec![Content::text(output)]),
                execution: Some(execution),
            });
        }

        // Definition search → Tantivy symbol mode
        match &workspace_target {
            WorkspaceTarget::Primary => {
                let primary_id = handler.require_primary_workspace_identity()?;
                if handler
                    .get_search_index_for_workspace(&primary_id)
                    .await?
                    .is_none()
                {
                    let message = "Definition search requires a Tantivy index for the current primary workspace. Run manage_workspace(operation=\"refresh\") first.";
                    return Ok(FastSearchExecution {
                        result: CallToolResult::text_content(vec![Content::text(message)]),
                        execution: None,
                    });
                }
            }
            WorkspaceTarget::Target(id) => {
                handler.get_database_for_workspace(id).await?;
                if handler.get_search_index_for_workspace(id).await?.is_none() {
                    let message = format!(
                        "Definition search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.",
                        id, id
                    );
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
                let message = format!(
                    "Definition search requires a Tantivy index for workspace '{}'. Run manage_workspace(operation=\"refresh\", workspace_id=\"{}\") first.",
                    target_workspace_id, target_workspace_id
                );
                return Ok(FastSearchExecution {
                    result: CallToolResult::text_content(vec![Content::text(message)]),
                    execution: None,
                });
            }
        }

        let execution = execution::execute_search(
            execution::SearchExecutionParams {
                query: &self.query,
                language: &self.language,
                file_pattern: &self.file_pattern,
                limit: self.limit,
                search_target: &self.search_target,
                context_lines: self.context_lines,
                exclude_tests: self.exclude_tests,
            },
            &execution_workspaces,
            handler,
        )
        .await?;

        let symbols = execution.definition_symbols();

        let optimized = OptimizedResponse::with_total(symbols, execution.total_results);

        if optimized.results.is_empty() {
            let message = format!(
                "No results found for: '{}'\n\
                Try search_target=\"content\" for line-level search, or a broader query",
                self.query
            );
            return Ok(FastSearchExecution {
                result: CallToolResult::text_content(vec![Content::text(message)]),
                execution: Some(execution),
            });
        }

        // Locations-only mode: skip code context entirely (70-90% token savings)
        if self.return_format == "locations" {
            let mut locations_output = formatting::format_locations_only(&self.query, &optimized);
            if execution.relaxed {
                locations_output = format!(
                    "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                    locations_output
                );
            }
            return Ok(FastSearchExecution {
                result: CallToolResult::text_content(vec![Content::text(locations_output)]),
                execution: Some(execution),
            });
        }

        // Definition search: use promoted formatting (exact matches get "Definition found:" header)
        let lean_output = formatting::format_definition_search_results(&self.query, &optimized);

        // Prepend relaxed-match indicator when OR fallback was used
        let lean_output = if execution.relaxed {
            format!(
                "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                lean_output
            )
        } else {
            lean_output
        };

        debug!(
            "✅ Returning lean search results ({} chars, {} results, relaxed: {})",
            lean_output.len(),
            optimized.results.len(),
            execution.relaxed,
        );
        Ok(FastSearchExecution {
            result: CallToolResult::text_content(vec![Content::text(lean_output)]),
            execution: Some(execution),
        })
    }

    /// Resolve workspace filtering parameter to a WorkspaceTarget.
    ///
    /// Delegates to the canonical `resolve_workspace_filter` in `resolution.rs`.
    /// FastSearchTool keeps this as a convenience method since it accesses `self.workspace`.
    async fn resolve_workspace_filter(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<WorkspaceTarget> {
        crate::tools::navigation::resolution::resolve_workspace_filter(
            self.workspace.as_deref(),
            handler,
        )
        .await
    }
}
