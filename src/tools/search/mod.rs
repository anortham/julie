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
pub use self::trace::{
    FilePatternDiagnostic, HintKind, SearchExecutionResult, SearchHit, SearchTrace, ZeroHitReason,
};
pub use self::types::{LineMatch, LineMatchStrategy};

// Internal modules
pub(crate) mod execution;
pub(crate) mod formatting; // Exposed for testing
pub(crate) mod hint_formatter;
pub(crate) mod input_diagnostics;
pub(crate) mod line_mode;
pub(crate) mod line_output;
mod nl_embeddings;
pub(crate) mod query;
pub mod query_preprocessor; // Public for testing
pub mod text_search;
pub(crate) mod trace;
mod types;

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use schemars::JsonSchema;
use serde::de::{Deserializer, Error as DeError, IntoDeserializer};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::health::SystemStatus;
use crate::tools::navigation::resolution::WorkspaceTarget;
use crate::tools::shared::OptimizedResponse;

const MIN_LIMIT: u32 = 1;
const MAX_LIMIT: u32 = 500;

//******************//
//   Search Tools   //
//******************//

#[derive(Debug, Serialize, JsonSchema)]
/// Search code and symbols using unified code-aware full-text search. Supports multi-word queries with AND/OR logic, exact symbol name matches, file-path fragments, and conceptual semantic search — all in one call.
pub struct FastSearchTool {
    /// Search query. Exact symbol names, file path fragments, and natural-language descriptions all work. Too many results? Add file_pattern or language filter. Zero results? Run manage_workspace(operation="index")
    pub query: String,
    /// Language filter: "rust", "typescript", "javascript", "python", "java", "csharp", "vbnet", "php", "ruby", "swift", "kotlin", "scala", "go", "c", "cpp", "lua", "qml", "r", "sql", "html", "css", "vue", "bash", "gdscript", "dart", "zig"
    #[serde(default)]
    pub language: Option<String>,
    /// File pattern filter (glob syntax)
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum results (default: 10, range: 1-500)
    #[serde(
        default = "default_limit",
        deserialize_with = "deserialize_limit_lenient_clamped"
    )]
    pub limit: u32,
    /// Context lines before/after a match (default: 1)
    #[serde(
        default = "default_context_lines",
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_u32_lenient"
    )]
    pub context_lines: Option<u32>,
    /// Exclude test symbols from results.
    /// Default: auto (excludes for NL queries, includes for symbol searches).
    /// Set explicitly to override.
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_bool_lenient"
    )]
    pub exclude_tests: Option<bool>,
    /// Workspace filter: "primary" (default) or a workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
    /// Return format: "full" (default, code context and rich summaries) or "locations" (file:line only)
    #[serde(default = "default_return_format")]
    pub return_format: String,
}

#[derive(Deserialize)]
struct FastSearchToolSerde {
    query: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    file_pattern: Option<String>,
    #[serde(
        default = "default_limit",
        deserialize_with = "deserialize_limit_lenient_clamped"
    )]
    limit: u32,
    #[serde(default, deserialize_with = "deserialize_presence_tracked_option_u32")]
    context_lines: Option<Option<u32>>,
    #[serde(
        default,
        deserialize_with = "crate::utils::serde_lenient::deserialize_option_bool_lenient"
    )]
    exclude_tests: Option<bool>,
    #[serde(default = "default_workspace")]
    workspace: Option<String>,
    #[serde(default = "default_return_format")]
    return_format: String,
}

impl<'de> Deserialize<'de> for FastSearchTool {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = FastSearchToolSerde::deserialize(deserializer)?;
        let context_lines = match raw.context_lines {
            Some(value) => value,
            None => default_context_lines(),
        };

        Ok(Self {
            query: raw.query,
            language: raw.language,
            file_pattern: raw.file_pattern,
            limit: raw.limit,
            context_lines,
            exclude_tests: raw.exclude_tests,
            workspace: raw.workspace,
            return_format: raw.return_format,
        })
    }
}

fn default_limit() -> u32 {
    10 // Reduced from 15 with enhanced scoring (better quality = fewer results needed)
}

fn clamp_limit(limit: u32) -> u32 {
    limit.clamp(MIN_LIMIT, MAX_LIMIT)
}

fn deserialize_limit_lenient_clamped<'de, D>(deserializer: D) -> std::result::Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let limit = crate::utils::serde_lenient::deserialize_u32_lenient(deserializer)?;
    Ok(clamp_limit(limit))
}

fn default_workspace() -> Option<String> {
    Some("primary".to_string())
}
fn default_context_lines() -> Option<u32> {
    Some(1) // 1 before + match + 1 after = 3 total lines (minimal context)
}
fn default_return_format() -> String {
    "full".to_string()
}

fn deserialize_presence_tracked_option_u32<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Option<u32>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(value) => {
            let parsed = crate::utils::serde_lenient::deserialize_option_u32_lenient(
                value.into_deserializer(),
            )
            .map_err(D::Error::custom)?;
            Ok(Some(parsed))
        }
    }
}

impl Default for FastSearchTool {
    fn default() -> Self {
        Self {
            query: String::new(),
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

pub struct FastSearchExecution {
    pub result: CallToolResult,
    pub execution: Option<SearchExecutionResult>,
}

impl FastSearchTool {
    pub(crate) fn effective_limit(&self) -> u32 {
        clamp_limit(self.limit)
    }

    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        self.execute_with_trace(handler).await.map(|run| run.result)
    }

    pub async fn execute_with_trace(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<FastSearchExecution> {
        debug!("🔍 Fast search (unified): {}", self.query);

        // Validate file_pattern syntax and emit early diagnostic if it looks like
        // whitespace-separated globs.
        if let Some(diagnostic) =
            input_diagnostics::build_request_level_file_pattern_diagnostic(&self.query, self.file_pattern.as_deref())
        {
            return Ok(diagnostic);
        }

        // Resolve workspace target once (used for health check and search routing)
        let workspace_target = self.resolve_workspace_filter(handler).await?;
        let effective_limit = self.effective_limit();

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

                    // Probe-only: legacy method is intentional here. The pooled
                    // accessor requires workspace_pool membership; this probe
                    // just checks DB existence to choose the right error message.
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
                    // Probe-only: see note above; legacy method does file-level
                    // probing without requiring workspace_pool membership.
                    if handler
                        .get_database_for_workspace(target_workspace_id)
                        .await
                        .is_ok()
                        && handler
                            .get_search_index_for_workspace(target_workspace_id)
                            .await?
                            .is_none()
                    {
                        let message =
                            missing_index_message(Some(target_workspace_id.as_str()));
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(message)]),
                            execution: None,
                        });
                    }
                }

                let message = "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first.";
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

        // Unified path: all queries go through execute_search_unified.
        let execution_workspaces = match &workspace_target {
            WorkspaceTarget::Primary => vec![execution::SearchExecutionWorkspace::primary(
                handler.require_primary_workspace_identity()?,
            )],
            WorkspaceTarget::Target(id) => {
                vec![execution::SearchExecutionWorkspace::target(id.clone())]
            }
        };

        // Require Tantivy index.
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
                // Probe-only: legacy method intentionally used here.
                handler.get_database_for_workspace(id).await?;
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
            // Probe-only: legacy method intentionally used here.
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
            },
            &execution_workspaces,
            handler,
        )
        .await?;

        // T12 fix: the unified search returns mixed file+symbol hits.  Pulling
        // only `definition_symbols()` silently drops file rows, which is what
        // caused the Phase 2 file/path-search regression (Eros bakeoff −46).
        // Render the full `execution.hits` slice — `format_unified_search_results`
        // handles both kinds and preserves rank order.
        let query_lower = self.query.to_lowercase();
        execution.trace.definition_exact_match = execution.hits.iter().any(|hit| {
            if let Some(symbol) = hit.as_symbol() {
                formatting::is_definition_name_match(&symbol.name, &query_lower)
            } else {
                false
            }
        });

        if execution.hits.is_empty() {
            // Prefer the targeted content zero-hit hint that
            // `execute_search_unified` already computed and stamped on the
            // trace (OutOfScopeContentHint, FilePatternSyntaxHint, etc.).
            // Fall back to the generic "no results" message only when no
            // hint was produced.
            let message = if let Some((_hint_kind, hint_text)) =
                hint_formatter::build_content_zero_hit_hint(
                    &self.query,
                    self.file_pattern.as_deref(),
                    self.language.as_deref(),
                    self.exclude_tests,
                    execution.trace.zero_hit_reason.as_ref(),
                    execution.trace.file_pattern_diagnostic.as_ref(),
                )
            {
                hint_text
            } else {
                format!(
                    "No results found for: '{}'\n\
                    Try a broader query, or add a file_pattern or language filter",
                    self.query
                )
            };
            return Ok(FastSearchExecution {
                result: CallToolResult::text_content(vec![Content::text(message)]),
                execution: Some(execution),
            });
        }

        // Locations-only mode: skip code context entirely (70-90% token savings)
        if self.return_format == "locations" {
            // T8 follow-up: when locations mode is requested AND the query is
            // a content match (no exact-name symbol matches it), supplement
            // the unified result with line-mode line numbers so callers see
            // the actual matching line rather than the enclosing symbol's
            // declaration line.  This restores the behaviour of the old
            // `execute_content_search` locations path.
            let has_exact_name_match = execution.hits.iter().any(|hit| {
                if let Some(symbol) = hit.as_symbol() {
                    formatting::is_definition_name_match(&symbol.name, &query_lower)
                } else {
                    false
                }
            });
            if !has_exact_name_match {
                if let Ok(line_locations_output) = self
                    .try_line_mode_locations(handler, &workspace_target)
                    .await
                {
                    if let Some(locations_text) = line_locations_output {
                        let final_text = if execution.relaxed {
                            format!(
                                "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                                locations_text
                            )
                        } else {
                            locations_text
                        };
                        return Ok(FastSearchExecution {
                            result: CallToolResult::text_content(vec![Content::text(final_text)]),
                            execution: Some(execution),
                        });
                    }
                }
            }

            // T12 fix: render mixed-kind hits via the unified locations formatter
            // so file rows appear alongside symbol rows in rank order.
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
            return Ok(FastSearchExecution {
                result: CallToolResult::text_content(vec![Content::text(locations_output)]),
                execution: Some(execution),
            });
        }

        // T12 fix: render mixed-kind hits via the unified formatter so file rows
        // (kind == "file") appear in the output alongside symbol rows.  Without
        // this, path-shaped queries silently dropped their target file row at
        // the formatter boundary, causing the Phase 2 file/path-search regression.
        let lean_output = formatting::format_unified_search_results(
            &self.query,
            &execution.hits,
            execution.total_results,
        );

        // Prepend relaxed-match indicator when OR fallback was used
        let lean_output = if execution.relaxed {
            format!(
                "NOTE: Relaxed search (showing partial matches — no results matched all terms)\n\n{}",
                lean_output
            )
        } else {
            lean_output
        };

        // Prepend scope-rescue header when execute_search_unified relaxed the
        // file_pattern.  Mirrors the legacy line-mode rescue behaviour so
        // callers see "0 in scope; here is what exists outside scope" before
        // the actual results.
        let lean_output = if execution.trace.scope_relaxed
            && let Some(original_pattern) = execution.trace.original_file_pattern.as_deref()
        {
            // Scope-rescue header reports user-visible result count.  The
            // unified formatter groups by file path, so the user perceives one
            // group per distinct file rather than one entry per raw hit (which
            // double-counts file+symbol pairs from the same path).
            let distinct_files: std::collections::HashSet<&str> =
                execution.hits.iter().map(|hit| hit.file.as_str()).collect();
            format!(
                "{}\n\n{}",
                hint_formatter::build_scope_rescue_header(
                    original_pattern,
                    distinct_files.len(),
                ),
                lean_output,
            )
        } else {
            lean_output
        };

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

    /// Try to produce content-style locations output (file:line per match) by
    /// running line-mode scanning.  Used by `return_format == "locations"` when
    /// the unified search did not find an exact-name symbol match — in that
    /// case the file:line of the actual content match is more useful than the
    /// declaration line of an enclosing symbol.
    ///
    /// Returns `Ok(Some(text))` on success, `Ok(None)` if line-mode produced
    /// zero matches (caller falls back to symbol-locations output).  Errors
    /// bubble up so the caller can choose to fall back gracefully.
    async fn try_line_mode_locations(
        &self,
        handler: &JulieServerHandler,
        workspace_target: &WorkspaceTarget,
    ) -> Result<Option<String>> {
        let effective_limit = self.effective_limit();
        let line_result = line_mode::line_mode_matches(
            &self.query,
            &self.language,
            &self.file_pattern,
            effective_limit,
            self.exclude_tests,
            workspace_target,
            handler,
        )
        .await?;

        if line_result.matches.is_empty() {
            return Ok(None);
        }

        // Workspace label for SearchHit construction (used only for telemetry
        // round-tripping; locations output does not render it).
        let workspace_label = match workspace_target {
            WorkspaceTarget::Primary => handler
                .require_primary_workspace_identity()
                .unwrap_or_else(|_| "primary".to_string()),
            WorkspaceTarget::Target(id) => id.clone(),
        };

        let language_label = match &self.language {
            Some(lang) => lang.clone(),
            None => "rust".to_string(),
        };

        let hits: Vec<crate::tools::search::trace::SearchHit> = line_result
            .matches
            .into_iter()
            .map(|line_match| {
                crate::tools::search::trace::SearchHit::from_line_match(
                    line_match,
                    workspace_label.clone(),
                    language_label.clone(),
                    0.0_f32,
                )
            })
            .collect();

        let total_results = hits.len();
        let optimized = OptimizedResponse::with_total(hits, total_results);
        Ok(Some(formatting::format_content_locations_only(
            &self.query,
            &optimized,
        )))
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
