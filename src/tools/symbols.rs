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
use tracing::{debug, info, warn};

use crate::extractors::base::Visibility;
use crate::handler::JulieServerHandler;
use crate::utils::context_truncation::ContextTruncator;
use crate::utils::token_estimation::TokenEstimator;

fn default_max_depth() -> u32 {
    1
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

    /// Include full code bodies for symbols (default: false)
    /// When true, shows complete function/class/method code
    /// 70-90% token savings vs reading entire file
    #[serde(default)]
    pub include_body: bool,

    /// Filter to specific symbol(s) by name (optional)
    /// Example: "UserService" to show only UserService class
    /// Supports partial matching (case-insensitive)
    #[serde(default)]
    pub target: Option<String>,

    /// Reading mode: "structure" (default), "minimal", "full"
    /// - structure: No bodies, structure only (current behavior)
    /// - minimal: Bodies for top-level symbols only
    /// - full: Bodies for all symbols including nested methods
    #[serde(default)]
    pub mode: Option<String>,
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
                    "ℹ️ No symbols matching '{}' found in: {}\n\n\
                     💡 Try without target filter to see all available symbols",
                    self.target.as_ref().unwrap(),
                    self.file_path
                );
                return Ok(CallToolResult::text_content(vec![TextContent::from(
                    self.optimize_response(&message),
                )]));
            }
        }

        // Read file content if bodies are needed (Smart Read: on-demand extraction)
        let file_content = if self.include_body {
            match tokio::fs::read_to_string(&absolute_path).await {
                Ok(content) => Some(content),
                Err(e) => {
                    warn!("⚠️  Could not read file for body extraction: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Determine effective reading mode
        let effective_mode = self.mode.as_deref().unwrap_or("structure");
        debug!(
            "🎯 Smart Read mode: {} (include_body: {}, target: {:?})",
            effective_mode, self.include_body, self.target
        );

        // Build hierarchical symbol tree respecting max_depth
        let mut output = String::new();

        // Smart target filtering: Search ALL symbols, then show parent hierarchy
        let top_level_symbols: Vec<_> = if let Some(ref target) = self.target {
            let target_lower = target.to_lowercase();

            // Find all symbols that match the target (including nested ones)
            let matching_symbols: Vec<_> = all_symbols
                .iter()
                .filter(|s| s.name.to_lowercase().contains(&target_lower))
                .collect();

            if matching_symbols.is_empty() {
                vec![] // No matches
            } else {
                // For each matching symbol, find its root parent (or use itself if top-level)
                let mut root_parents = std::collections::HashSet::new();

                for symbol in matching_symbols {
                    let root = Self::find_root_parent(symbol, &all_symbols);
                    root_parents.insert(root.id.clone());
                }

                // Get the actual top-level symbols to display
                all_symbols
                    .iter()
                    .filter(|s| s.parent_id.is_none() && root_parents.contains(&s.id))
                    .collect()
            }
        } else {
            // No filter - show all top-level symbols
            all_symbols
                .iter()
                .filter(|s| s.parent_id.is_none())
                .collect()
        };

        let symbol_count_text = if let Some(ref t) = self.target {
            format!(
                "📄 **{}** ({} symbols matching '{}')\n\n",
                self.file_path,
                top_level_symbols.len(),
                t
            )
        } else {
            format!(
                "📄 **{}** ({} symbols)\n\n",
                self.file_path,
                all_symbols.len()
            )
        };
        output.push_str(&symbol_count_text);

        let top_level_count = top_level_symbols.len();
        debug!("Found {} top-level symbols", top_level_count);

        for symbol in top_level_symbols {
            self.format_symbol(
                &mut output,
                symbol,
                &all_symbols, // Pass ALL symbols so children can be found (bug fix)
                0,
                self.max_depth,
                file_content.as_deref(),
                effective_mode,
            );
        }

        // Add summary statistics
        output.push_str("\n---\n\n");
        output.push_str(&format!("**Summary:**\n"));
        output.push_str(&format!("• Total symbols: {}\n", all_symbols.len()));
        output.push_str(&format!("• Top-level: {}\n", top_level_count));

        // Count by kind
        let mut kind_counts = std::collections::HashMap::new();
        for symbol in &all_symbols {
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
        file_content: Option<&str>,
        mode: &str,
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
        output.push_str(&format!("{}{} **{}**", indent, icon, symbol.name));

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

        // Smart Read: Extract and display code body if requested
        let should_show_body = match mode {
            "structure" => false, // Default: structure only, no bodies
            "minimal" => current_depth == 0 && file_content.is_some(), // Top-level only
            "full" => file_content.is_some(), // All symbols
            _ => false,
        };

        if should_show_body && self.include_body {
            if let Some(content) = file_content {
                let body = self.extract_symbol_body(content, symbol);
                if let Some(body_text) = body {
                    // Format body with indentation and syntax highlighting hint
                    let body_indent = "  ".repeat((current_depth + 1) as usize);
                    output.push_str(&format!("{}```\n", body_indent));
                    for line in body_text.lines() {
                        output.push_str(&format!("{}{}\n", body_indent, line));
                    }
                    output.push_str(&format!("{}```\n", body_indent));
                }
            }
        }

        // Recurse into children if within max_depth
        if current_depth < max_depth {
            let children: Vec<_> = all_symbols
                .iter()
                .filter(|s| s.parent_id.as_ref() == Some(&symbol.id))
                .collect();

            for child in children {
                self.format_symbol(
                    output,
                    child,
                    all_symbols,
                    current_depth + 1,
                    max_depth,
                    file_content,
                    mode,
                );
            }
        } else if current_depth == max_depth {
            // Indicate there are more children not shown
            let child_count = all_symbols
                .iter()
                .filter(|s| s.parent_id.as_ref() == Some(&symbol.id))
                .count();
            if child_count > 0 {
                output.push_str(&format!(
                    "{}  └─ ... {} more nested symbols (increase max_depth to see)\n",
                    indent, child_count
                ));
            }
        }
    }

    /// Extract complete symbol body from file content (Smart Read core logic)
    /// Uses tree-sitter-validated line boundaries for clean extraction
    /// Now with smart truncation to preserve structure while limiting tokens
    fn extract_symbol_body(
        &self,
        content: &str,
        symbol: &crate::extractors::Symbol,
    ) -> Option<String> {
        let lines: Vec<&str> = content.lines().collect();

        // Use 1-based line numbers from symbol (tree-sitter convention)
        let start_line = symbol.start_line.saturating_sub(1) as usize; // Convert to 0-based
        let end_line =
            (symbol.end_line.saturating_sub(1) as usize).min(lines.len().saturating_sub(1));

        if start_line >= lines.len() {
            warn!(
                "⚠️  Symbol start line {} exceeds file length {}",
                symbol.start_line,
                lines.len()
            );
            return None;
        }

        // Extract complete symbol body (respects tree-sitter AST boundaries)
        let body_lines = &lines[start_line..=end_line];

        // Smart Read insight: Remove common indentation for cleaner display
        let min_indent = body_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.chars().take_while(|c| c.is_whitespace()).count())
            .min()
            .unwrap_or(0);

        let clean_body: Vec<String> = body_lines
            .iter()
            .map(|line| {
                if line.len() > min_indent {
                    line[min_indent..].to_string()
                } else {
                    line.to_string()
                }
            })
            .collect();

        // Apply smart truncation if body is large (>50 lines)
        // This preserves structure while limiting token usage
        if clean_body.len() > 50 {
            let truncator = ContextTruncator::new();
            Some(truncator.smart_truncate(&clean_body, 40)) // Limit to ~40 lines, preserving structure
        } else {
            Some(clean_body.join("\n"))
        }
    }
}

// Implement token optimization trait
impl GetSymbolsTool {
    /// Find the root parent of a symbol (walk up the parent chain to top-level)
    fn find_root_parent<'a>(
        symbol: &'a crate::extractors::Symbol,
        all_symbols: &'a [crate::extractors::Symbol],
    ) -> &'a crate::extractors::Symbol {
        let mut current = symbol;

        // Walk up the parent chain until we find a top-level symbol
        while let Some(ref parent_id) = current.parent_id {
            // Find the parent symbol
            if let Some(parent) = all_symbols.iter().find(|s| &s.id == parent_id) {
                current = parent;
            } else {
                // Parent not found, return current (shouldn't happen in valid data)
                break;
            }
        }

        current
    }

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
