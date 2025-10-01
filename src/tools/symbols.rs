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

use crate::extractors::base::Visibility;
use crate::handler::JulieServerHandler;
use crate::utils::token_estimation::TokenEstimator;

fn default_max_depth() -> u32 {
    1
}

//**********************//
//   Get Symbols Tool   //
//**********************//

#[mcp_tool(
    name = "get_symbols",
    description = "GET FILE SKELETON - See all symbols in a file without reading full content (saves context)",
    title = "Get File Symbol Overview",
    idempotent_hint = true,
    destructive_hint = false,
    open_world_hint = false,
    read_only_hint = true,
    meta = r#"{"category": "navigation", "performance": "instant"}"#
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
}

impl GetSymbolsTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        info!("📋 Getting symbols for file: {} (depth: {})", self.file_path, self.max_depth);

        // Get the workspace and database
        let workspace = handler
            .get_workspace()
            .await?
            .ok_or_else(|| anyhow::anyhow!("No workspace initialized. Run 'manage_workspace index' first"))?;

        let db = workspace
            .db
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No database available"))?;

        // Query symbols for this file
        let db_lock = db.lock().await;
        let symbols = db_lock
            .get_symbols_for_file(&self.file_path)
            .map_err(|e| anyhow::anyhow!("Failed to get symbols: {}", e))?;

        if symbols.is_empty() {
            let message = format!(
                "ℹ️ No symbols found in: {}\n\n\
                 Possible reasons:\n\
                 • File not indexed yet (check if file exists)\n\
                 • File contains no symbols (empty or just comments)\n\
                 • File type not supported for symbol extraction\n\n\
                 💡 Try running fast_search to find the file first",
                self.file_path
            );
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&message),
            )]));
        }

        // Build hierarchical symbol tree respecting max_depth
        let mut output = String::new();
        output.push_str(&format!("📄 **{}** ({} symbols)\n\n", self.file_path, symbols.len()));

        // Group symbols by hierarchy
        let top_level_symbols: Vec<_> = symbols
            .iter()
            .filter(|s| s.parent_id.is_none())
            .collect();

        let top_level_count = top_level_symbols.len();
        debug!("Found {} top-level symbols", top_level_count);

        for symbol in top_level_symbols {
            self.format_symbol(&mut output, symbol, &symbols, 0, self.max_depth);
        }

        // Add summary statistics
        output.push_str("\n---\n\n");
        output.push_str(&format!("**Summary:**\n"));
        output.push_str(&format!("• Total symbols: {}\n", symbols.len()));
        output.push_str(&format!("• Top-level: {}\n", top_level_count));

        // Count by kind
        let mut kind_counts = std::collections::HashMap::new();
        for symbol in &symbols {
            *kind_counts.entry(symbol.kind.to_string()).or_insert(0) += 1;
        }
        for (kind, count) in kind_counts.iter() {
            output.push_str(&format!("• {}: {}\n", kind, count));
        }

        Ok(CallToolResult::text_content(vec![TextContent::from(
            self.optimize_response(&output),
        )]))
    }

    /// Format a symbol and its children recursively
    fn format_symbol(
        &self,
        output: &mut String,
        symbol: &crate::extractors::Symbol,
        all_symbols: &[crate::extractors::Symbol],
        current_depth: u32,
        max_depth: u32,
    ) {
        // Indentation based on depth
        let indent = "  ".repeat(current_depth as usize);

        // Symbol icon based on kind
        let icon = match symbol.kind {
            crate::extractors::SymbolKind::Class => "🏛️",
            crate::extractors::SymbolKind::Function => "⚡",
            crate::extractors::SymbolKind::Method => "🔧",
            crate::extractors::SymbolKind::Variable => "📦",
            crate::extractors::SymbolKind::Constant => "💎",
            crate::extractors::SymbolKind::Interface => "🔌",
            crate::extractors::SymbolKind::Enum => "🎯",
            crate::extractors::SymbolKind::Struct => "🏗️",
            _ => "•",
        };

        // Format symbol line
        output.push_str(&format!(
            "{}{} **{}**",
            indent, icon, symbol.name
        ));

        // Add signature if available
        if let Some(ref sig) = symbol.signature {
            if !sig.is_empty() && sig != &symbol.name {
                output.push_str(&format!(" `{}`", sig));
            }
        }

        // Add location
        output.push_str(&format!(" *(:{})*", symbol.start_line));

        // Add visibility if non-public
        if let Some(ref vis) = symbol.visibility {
            match vis {
                Visibility::Private | Visibility::Protected => {
                    output.push_str(&format!(" [{}]", vis));
                }
                _ => {}
            }
        }

        output.push('\n');

        // Recurse into children if within max_depth
        if current_depth < max_depth {
            let children: Vec<_> = all_symbols
                .iter()
                .filter(|s| s.parent_id.as_ref() == Some(&symbol.id))
                .collect();

            for child in children {
                self.format_symbol(output, child, all_symbols, current_depth + 1, max_depth);
            }
        } else if current_depth == max_depth {
            // Indicate there are more children not shown
            let child_count = all_symbols
                .iter()
                .filter(|s| s.parent_id.as_ref() == Some(&symbol.id))
                .count();
            if child_count > 0 {
                output.push_str(&format!("{}  └─ ... {} more nested symbols (increase max_depth to see)\n", indent, child_count));
            }
        }
    }
}

// Implement token optimization trait
impl GetSymbolsTool {
    fn optimize_response(&self, response: &str) -> String {
        let estimator = TokenEstimator::new();
        let tokens = estimator.estimate_string(response);

        // Target 15000 tokens for symbol lists (reasonable for overview)
        if tokens <= 15000 {
            response.to_string()
        } else {
            // Simple truncation with notice
            let chars_per_token = response.len() / tokens.max(1);
            let target_chars = chars_per_token * 15000;
            let truncated = &response[..target_chars.min(response.len())];
            format!("{}\n\n... (truncated for context limits)", truncated)
        }
    }
}
