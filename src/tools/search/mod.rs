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
pub(crate) mod formatting; // Exposed for testing
mod line_mode;
pub(crate) mod query;
pub mod query_preprocessor; // Public for testing
pub mod text_search;
mod types;

use std::sync::Arc;

use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::health::SystemStatus;
use crate::search::index::SearchFilter;
use crate::tools::navigation::resolution::WorkspaceTarget;
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
    #[serde(default = "default_limit", deserialize_with = "crate::utils::serde_lenient::deserialize_u32_lenient")]
    pub limit: u32,
    /// Context lines before/after match (default: 1)
    #[serde(default = "default_context_lines", deserialize_with = "crate::utils::serde_lenient::deserialize_option_u32_lenient")]
    pub context_lines: Option<u32>,
    /// Workspace filter: "primary" (default), workspace ID, or "all" (daemon mode: search all projects)
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
        debug!(
            "🔍 Fast search: {} (target: {})",
            self.query, self.search_target
        );

        // Resolve workspace target once (used for health check and search routing)
        let workspace_target = self.resolve_workspace_filter(handler).await?;

        // --- Federated search: workspace="all" ---
        if matches!(workspace_target, WorkspaceTarget::All) {
            return self.federated_search(handler).await;
        }

        // --- Single-workspace search ---

        // Extract workspace ID for health check
        let target_workspace_id = match &workspace_target {
            WorkspaceTarget::Reference(id) => Some(id.clone()),
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
        // Convert WorkspaceTarget to Option<Vec<String>> for text_search_impl
        let workspace_ids = match workspace_target {
            WorkspaceTarget::Primary => {
                // Resolve the actual primary workspace ID for Tantivy filtering
                if let Some(workspace) = handler.get_workspace().await? {
                    let registry_service =
                        crate::workspace::registry_service::WorkspaceRegistryService::new(
                            workspace.root.clone(),
                        );
                    match registry_service.get_primary_workspace_id().await? {
                        Some(id) => Some(vec![id]),
                        None => None,
                    }
                } else {
                    None
                }
            }
            WorkspaceTarget::Reference(id) => Some(vec![id]),
            WorkspaceTarget::All => unreachable!("All handled above"),
        };
        let (symbols, relaxed) = text_search::text_search_impl(
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

        let mut optimized = OptimizedResponse::new(symbols);
        optimized.results.truncate(self.limit as usize);

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

        // Prepend relaxed-match indicator when OR fallback was used
        let lean_output = if relaxed {
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
            relaxed,
        );
        Ok(CallToolResult::text_content(vec![Content::text(
            lean_output,
        )]))
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

    /// Execute a federated search across all daemon-registered workspaces.
    ///
    /// Requires daemon mode (`handler.daemon_state` must be `Some`).
    /// Collects all `Ready` workspaces, fans out the search in parallel,
    /// and merges results with RRF. Output is tagged with `[project: name]`.
    async fn federated_search(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        use crate::daemon_state::WorkspaceLoadStatus;
        use crate::tools::federation::search::{
            WorkspaceSearchEntry, federated_content_search, federated_symbol_search,
        };

        // Require daemon mode
        let daemon_state = handler.daemon_state.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Cross-project search (workspace=\"all\") requires daemon mode.\n\
                 In stdio mode, search one workspace at a time using 'primary' or a workspace ID."
            )
        })?;

        // Read-lock DaemonState, collect Ready workspaces, then drop lock
        let entries: Vec<WorkspaceSearchEntry> = {
            let state = daemon_state.read().await;
            state
                .workspaces
                .iter()
                .filter(|(_, loaded)| loaded.status == WorkspaceLoadStatus::Ready)
                .filter_map(|(ws_id, loaded)| {
                    let search_index = loaded.workspace.search_index.as_ref()?;
                    let project_name = loaded
                        .path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(ws_id)
                        .to_string();
                    Some(WorkspaceSearchEntry {
                        workspace_id: ws_id.clone(),
                        project_name,
                        search_index: Arc::clone(search_index),
                        db: loaded.workspace.db.clone(),
                    })
                })
                .collect()
        };

        if entries.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "No ready workspaces available for cross-project search.\n\
                 Register and index projects first using the daemon API.",
            )]));
        }

        debug!(
            "Federated search across {} workspaces: {:?}",
            entries.len(),
            entries.iter().map(|e| &e.project_name).collect::<Vec<_>>()
        );

        let filter = SearchFilter {
            language: self.language.clone(),
            file_pattern: self.file_pattern.clone(),
            ..Default::default()
        };
        let limit = self.limit as usize;

        let is_definition_search = self.search_target == "definitions";

        if is_definition_search {
            // --- Federated definition search ---
            let federated_results =
                federated_symbol_search(&self.query, &filter, limit, &entries).await?;

            // Convert to (Symbol, project_name) pairs
            let mut symbols = Vec::with_capacity(federated_results.len());
            let mut project_names = Vec::with_capacity(federated_results.len());
            for fr in federated_results {
                symbols.push(text_search::tantivy_symbol_to_symbol(fr.result));
                project_names.push(fr.project_name);
            }

            let symbols = formatting::truncate_code_context(symbols, self.context_lines);

            let mut optimized = OptimizedResponse::new(symbols);
            optimized.results.truncate(limit);
            project_names.truncate(limit);

            if optimized.results.is_empty() {
                let message = format!(
                    "No results found for: '{}' across all projects\n\
                     Try a broader search term or different keywords",
                    self.query
                );
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
            }

            let lean_output = formatting::format_federated_definition_results(
                &self.query,
                &optimized,
                &project_names,
            );

            debug!(
                "Returning federated definition results ({} chars, {} results)",
                lean_output.len(),
                optimized.results.len(),
            );
            Ok(CallToolResult::text_content(vec![Content::text(
                lean_output,
            )]))
        } else {
            // --- Federated content search ---
            let federated_results =
                federated_content_search(&self.query, &filter, limit, &entries).await?;

            let mut symbols = Vec::with_capacity(federated_results.len());
            let mut project_names = Vec::with_capacity(federated_results.len());
            for fr in federated_results {
                symbols.push(text_search::content_result_to_symbol(fr.result));
                project_names.push(fr.project_name);
            }

            let symbols = formatting::truncate_code_context(symbols, self.context_lines);

            let mut optimized = OptimizedResponse::new(symbols);
            optimized.results.truncate(limit);
            project_names.truncate(limit);

            if optimized.results.is_empty() {
                let message = format!(
                    "No results found for: '{}' across all projects\n\
                     Try a broader search term or different keywords",
                    self.query
                );
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
            }

            let lean_output = formatting::format_federated_lean_results(
                &self.query,
                &optimized,
                &project_names,
            );

            debug!(
                "Returning federated content results ({} chars, {} results)",
                lean_output.len(),
                optimized.results.len(),
            );
            Ok(CallToolResult::text_content(vec![Content::text(
                lean_output,
            )]))
        }
    }
}
