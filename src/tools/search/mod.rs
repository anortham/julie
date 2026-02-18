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
    PreprocessedQuery, QueryType, detect_query_type, preprocess_query,
    sanitize_query, validate_query,
};
pub use self::types::{LineMatch, LineMatchStrategy};

// Internal modules
pub(crate) mod formatting; // Exposed for testing
mod line_mode;
pub(crate) mod query;
pub mod query_preprocessor; // Public for testing
mod scoring;
pub mod text_search;
mod types;

use anyhow::Result;
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::health::SystemStatus;
use crate::tools::shared::OptimizedResponse;

//******************//
//   Search Tools   //
//******************//

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
/// Search code using text search with code-aware tokenization. Supports multi-word queries with AND/OR logic.
pub struct FastSearchTool {
    /// Search query (text or pattern)
    pub query: String,
    /// Search target: "content" (default) or "definitions"
    #[serde(default = "default_search_target")]
    pub search_target: String,
    /// Language filter: "rust", "typescript", "javascript", "python", "java", "csharp", "php", "ruby", "swift", "kotlin", "go", "c", "cpp", "lua", "qml", "r", "sql", "html", "css", "vue", "bash", "gdscript", "dart", "zig"
    #[serde(default)]
    pub language: Option<String>,
    /// File pattern filter (glob syntax)
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Maximum results (default: 10, range: 1-500)
    #[serde(default = "default_limit")]
    pub limit: u32,
    /// Context lines before/after match (default: 1)
    #[serde(default = "default_context_lines")]
    pub context_lines: Option<u32>,
    /// Workspace filter: "primary" (default) or workspace ID
    #[serde(default = "default_workspace")]
    pub workspace: Option<String>,
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

impl FastSearchTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        debug!("🔍 Fast search: {} (target: {})", self.query, self.search_target);

        // Determine target workspace for health check
        let target_workspace_id = if self.workspace.is_some() {
            self.resolve_workspace_filter(handler)
                .await?
                .and_then(|ids| ids.first().cloned())
        } else {
            None
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
                if use_line_mode {
                    debug!("Line-mode search before readiness; attempting SQLite fallback");
                } else {
                    let message = "Workspace not indexed yet. Run manage_workspace(operation=\"index\") first.";
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
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
        if use_line_mode {
            return line_mode::line_mode_search(
                &self.query,
                &self.language,
                &self.file_pattern,
                self.limit,
                &self.workspace,
                handler,
            )
            .await;
        }

        // Definition search → Tantivy symbol mode
        let workspace_ids = self.resolve_workspace_filter(handler).await?;
        let symbols = text_search::text_search_impl(
            &self.query,
            &self.language,
            &self.file_pattern,
            self.limit,
            workspace_ids,
            &self.search_target,
            self.context_lines,
            handler,
        )
        .await?;

        let symbols = formatting::truncate_code_context(symbols, self.context_lines);

        let confidence = scoring::calculate_search_confidence(&self.query, &symbols);
        let mut optimized = OptimizedResponse::new("fast_search", symbols, confidence);

        if let Some(insights) = scoring::generate_search_insights(&optimized.results, confidence) {
            optimized = optimized.with_insights(insights);
        }

        let next_actions = scoring::suggest_next_actions(&self.query, &optimized.results);
        optimized = optimized.with_next_actions(next_actions);

        optimized.optimize_for_tokens(Some(self.limit as usize));

        if optimized.results.is_empty() {
            let message = format!(
                "🔍 No results found for: '{}'\n\
                💡 Try a broader search term or different keywords",
                self.query
            );
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        // Definition search: use promoted formatting (exact matches get "Definition found:" header)
        let lean_output = formatting::format_definition_search_results(&self.query, &optimized);
            debug!(
                "✅ Returning lean search results ({} chars, {} results)",
                lean_output.len(),
                optimized.results.len()
            );
            Ok(CallToolResult::text_content(vec![Content::text(lean_output)]))
    }

    /// Resolve workspace filtering parameter to a list of workspace IDs
    async fn resolve_workspace_filter(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<Option<Vec<String>>> {
        let workspace_param = self.workspace.as_deref().unwrap_or("primary");

        match workspace_param {
            "all" => {
                // Multi-workspace search is not supported - architectural decision
                // Use ManageWorkspaceTool for listing/managing multiple workspaces
                Err(anyhow::anyhow!(
                    "Searching all workspaces is not supported. Search one workspace at a time.\n\
                     Use 'primary' (default) or specify a specific workspace ID.\n\
                     To list available workspaces, use ManageWorkspaceTool with operation='list'."
                ))
            }
            "primary" => {
                // Resolve primary workspace ID for precise workspace filtering
                let workspace = handler.get_workspace().await?;
                if let Some(workspace) = workspace {
                    let registry_service =
                        crate::workspace::registry_service::WorkspaceRegistryService::new(
                            workspace.root.clone(),
                        );
                    match registry_service.get_primary_workspace_id().await? {
                        Some(workspace_id) => {
                            debug!("🔍 Resolved primary workspace to ID: {}", workspace_id);
                            Ok(Some(vec![workspace_id]))
                        }
                        None => {
                            debug!("🔍 No primary workspace ID found, using fallback search");
                            Ok(None)
                        }
                    }
                } else {
                    debug!("🔍 No workspace available, using fallback search");
                    Ok(None)
                }
            }
            workspace_id => {
                // Validate the workspace ID exists
                if let Some(primary_workspace) = handler.get_workspace().await? {
                    let registry_service =
                        crate::workspace::registry_service::WorkspaceRegistryService::new(
                            primary_workspace.root.clone(),
                        );

                    // Check if it's a valid workspace ID
                    match registry_service.get_workspace(workspace_id).await? {
                        Some(_) => Ok(Some(vec![workspace_id.to_string()])),
                        None => {
                            // Invalid workspace ID - provide fuzzy match suggestion
                            let all_workspaces = registry_service.get_all_workspaces().await?;
                            let workspace_ids: Vec<&str> =
                                all_workspaces.iter().map(|w| w.id.as_str()).collect();

                            if let Some((best_match, distance)) =
                                crate::utils::string_similarity::find_closest_match(
                                    workspace_id,
                                    &workspace_ids,
                                )
                            {
                                // Only suggest if the distance is reasonable (< 50% of query length)
                                if distance < workspace_id.len() / 2 {
                                    return Err(anyhow::anyhow!(
                                        "Workspace '{}' not found. Did you mean '{}'?",
                                        workspace_id,
                                        best_match
                                    ));
                                }
                            }

                            // No close match found
                            Err(anyhow::anyhow!(
                                "Workspace '{}' not found. Use 'primary' or a valid workspace ID",
                                workspace_id
                            ))
                        }
                    }
                } else {
                    Err(anyhow::anyhow!(
                        "No primary workspace found. Initialize workspace first."
                    ))
                }
            }
        }
    }
}
