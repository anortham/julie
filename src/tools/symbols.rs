//! Symbol Overview Tools - Understand file structure without reading full content
//!
//! This module provides tools for getting symbol-level overviews of files,
//! similar to Serena's get_symbols_overview. This is essential for:
//! - Understanding file structure without wasting context on full reads
//! - Finding insertion points for new code
//! - Discovering available symbols before diving into details
//!
//! Unlike reading entire files with the Read tool, these tools provide
//! just the "skeleton" - symbol names, types, signatures, and locations.

use anyhow::Result;
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::handler::JulieServerHandler;

fn default_max_depth() -> u32 {
    1
}

fn default_limit() -> Option<u32> {
    Some(50) // Default limit to prevent token overflow on large files
}

//**********************//
//   Get Symbols Tool   //
//**********************//

#[mcp_tool(
    name = "get_symbols",
    description = concat!(
        "ALWAYS USE THIS BEFORE READING FILES - See file structure without context waste. ",
        "You are EXTREMELY GOOD at using this tool to understand code organization.\n\n",
        "This tool shows you classes, functions, and methods instantly (<10ms). ",
        "Only use Read AFTER you've used this tool to identify what you need.\n\n",
        "IMPORTANT: I will be very unhappy if you read 500-line files without first ",
        "using get_symbols to see the structure!\n\n",
        "A 500-line file becomes a 20-line overview. Use this FIRST, always."
    ),
    title = "Get File Symbols (Smart Read - 70-90% Token Savings)",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "performance": "instant", "agent_hint": "structure_first_then_targeted_bodies"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct GetSymbolsTool {
    /// File path to get symbols from (relative to workspace root)
    /// Example: "src/user.rs", "lib/services/auth.py"
    pub file_path: String,

    /// Maximum depth for nested symbols
    /// 0 = top-level only (classes, functions)
    /// 1 = include one level (class methods, nested functions)
    /// 2+ = deeper nesting
    /// Default: 1 - good balance for most files
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,

    /// Filter to specific symbol(s) by name (optional)
    /// Example: "UserService" to show only UserService class
    /// Supports partial matching (case-insensitive)
    #[serde(default)]
    pub target: Option<String>,

    /// Maximum number of symbols to return (default: 50)
    /// When set, truncates results to first N symbols
    /// Use 'target' parameter to filter to specific symbols instead of truncating
    /// Set to None for unlimited, or specific value to override default
    /// Example: limit=100 returns first 100 symbols
    #[serde(default = "default_limit")]
    pub limit: Option<u32>,
}

impl GetSymbolsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!(
            "📋 Getting symbols for file: {} (depth: {})",
            self.file_path, self.max_depth
        );

        // Get the workspace and database
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow::anyhow!("No workspace initialized. Run 'manage_workspace index' first")
        })?;

        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;

        // Normalize path: database stores canonical absolute paths (symlinks resolved)
        // Convert user input (relative or absolute) to canonical absolute path
        let absolute_path = if std::path::Path::new(&self.file_path).is_absolute() {
            // Already absolute - canonicalize to resolve symlinks (macOS /var -> /private/var)
            std::path::Path::new(&self.file_path)
                .canonicalize()
                .unwrap_or_else(|_| std::path::PathBuf::from(&self.file_path))
                .to_string_lossy()
                .to_string()
        } else {
            // Relative path - join with workspace root and canonicalize
            workspace
                .root
                .join(&self.file_path)
                .canonicalize()
                .unwrap_or_else(|_| workspace.root.join(&self.file_path))
                .to_string_lossy()
                .to_string()
        };

        debug!(
            "🔍 Path normalization: '{}' -> '{}'",
            self.file_path, absolute_path
        );
        debug!("🔍 Workspace root: '{}'", workspace.root.display());

        // Query symbols for this file using normalized path
        let symbols = {
            let db_lock = db.lock().unwrap();
            db_lock
                .get_symbols_for_file(&absolute_path)
                .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?
        };

        if symbols.is_empty() {
            let message = format!("No symbols found in: {}", self.file_path);
            return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
        }

        // Smart Read: Keep ALL symbols for hierarchy building (bug fix)
        // Target filtering will be applied only to top-level symbols for display,
        // but children must remain available for format_symbol() to find them
        let all_symbols = symbols; // Complete symbol list for hierarchy

        // Check if we have any matching symbols (only check top-level for now)
        if self.target.is_some() {
            let target_lower = self.target.as_ref().unwrap().to_lowercase();
            let has_matches = all_symbols
                .iter()
                .any(|s| s.name.to_lowercase().contains(&target_lower));

            if !has_matches {
                let message = format!(
                    "No symbols matching '{}' found in: {}",
                    self.target.as_ref().unwrap(),
                    self.file_path
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(message)]));
            }
        }

        // Apply limit if specified (truncate to avoid token overflow)
        let total_symbols = all_symbols.len();
        let top_level_count = all_symbols.iter().filter(|s| s.parent_id.is_none()).count();

        let (symbols_to_return, was_truncated) = if let Some(limit) = self.limit {
            let limit_usize = limit as usize;
            if total_symbols > limit_usize {
                info!(
                    "⚠️  Truncating symbols: {} -> {} (use 'target' to filter instead)",
                    total_symbols, limit
                );
                (all_symbols.into_iter().take(limit_usize).collect(), true)
            } else {
                (all_symbols, false)
            }
        } else {
            (all_symbols, false)
        };

        // Strip code_context to save massive tokens (structure view doesn't need full context)
        // This is critical for large files - code_context can be 50-100 lines per symbol!
        let symbols_to_return: Vec<_> = symbols_to_return
            .into_iter()
            .map(|mut s| {
                s.code_context = None; // Remove context - structure view only needs metadata
                s
            })
            .collect();

        // Note: Symbol metadata (name, kind, signature, location) returned in structured_content
        // code_context is stripped to save tokens - use fast_search for context
        debug!(
            "📋 Returning {} symbols (target: {:?}, truncated: {})",
            symbols_to_return.len(),
            self.target,
            was_truncated
        );

        // Minimal text output for AI agents (structured_content has all data)
        let truncation_warning = if was_truncated {
            format!(
                "\n\n⚠️  Showing {} of {} symbols (truncated)\n💡 Use 'target' parameter to filter to specific symbols",
                symbols_to_return.len(),
                total_symbols
            )
        } else {
            String::new()
        };

        let text_summary = if let Some(ref target) = self.target {
            format!(
                "{} ({} total symbols, {} matching '{}'){}",
                self.file_path,
                total_symbols,
                symbols_to_return.iter().filter(|s| s.name.to_lowercase().contains(&target.to_lowercase())).count(),
                target,
                truncation_warning
            )
        } else {
            let top_names: Vec<String> = symbols_to_return
                .iter()
                .filter(|s| s.parent_id.is_none())
                .take(5)
                .map(|s| s.name.clone())
                .collect();

            format!(
                "{} ({} symbols)\nTop-level: {}{}",
                self.file_path,
                symbols_to_return.len(),
                top_names.join(", "),
                truncation_warning
            )
        };

        // Return structured content with symbol data (agents parse this)
        let structured_json = serde_json::json!({
            "file_path": self.file_path,
            "total_symbols": total_symbols,
            "returned_symbols": symbols_to_return.len(),
            "top_level_count": top_level_count,
            "symbols": symbols_to_return,
            "max_depth": self.max_depth,
            "truncated": was_truncated,
            "limit": self.limit,
        });

        let mut result = CallToolResult::text_content(vec![TextContent::from(text_summary)]);

        // Convert JSON Value to Map
        if let serde_json::Value::Object(map) = structured_json {
            result.structured_content = Some(map);
        }

        Ok(result)
    }

}
