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
mod nl_embeddings;
pub(crate) mod query;
pub mod query_preprocessor; // Public for testing
pub mod text_search;
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
/// Search code using text search with code-aware tokenization. Supports multi-word queries with AND/OR logic.
pub struct FastSearchTool {
    /// Search query (text or pattern)
    pub query: String,
    /// Search target: "content" (default) or "definitions"
    #[serde(default = "default_search_target")]
    pub search_target: String,
    /// Language filter: "rust", "typescript", "javascript", "python", "java", "csharp", "php", "ruby", "swift", "kotlin", "scala", "go", "c", "cpp", "lua", "qml", "r", "sql", "html", "css", "vue", "bash", "gdscript", "dart", "zig"
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
    /// Exclude test symbols from results.
    /// Default: auto (excludes for NL queries, includes for definition searches).
    /// Set explicitly to override.
    #[serde(default)]
    pub exclude_tests: Option<bool>,
    /// Workspace filter: "primary" (default) or a reference workspace ID
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
                self.exclude_tests,
                &workspace_target,
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
        };
        let (symbols, relaxed) = text_search::text_search_impl(
            &self.query,
            &self.language,
            &self.file_pattern,
            self.limit,
            workspace_ids,
            &self.search_target,
            self.context_lines,
            self.exclude_tests,
            handler,
        )
        .await?;

        let symbols = formatting::truncate_code_context(symbols, self.context_lines);

        let mut optimized = OptimizedResponse::new(symbols);
        optimized.results.truncate(self.limit as usize);

        if optimized.results.is_empty() {
            let message = format!(
                "🔍 No results found for: '{}'\n\
                💡 Try search_target=\"content\" for line-level search, or a broader query",
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

}
