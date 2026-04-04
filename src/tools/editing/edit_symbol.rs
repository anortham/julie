//! edit_symbol tool: symbol-aware editing using Julie's indexed boundaries.
//!
//! The agent references a symbol by name. Julie looks up its location in the
//! index, then applies the edit. No file read required by the agent.

use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::handler::JulieServerHandler;
use crate::mcp_compat::CallToolResultExt;
use crate::utils::file_utils::secure_path_resolution;
use rmcp::model::{CallToolResult, Content};

use super::EditingTransaction;
use super::validation::{check_bracket_balance, format_unified_diff, should_check_balance};

fn default_dry_run() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditSymbolTool {
    /// Symbol name to edit (supports qualified names like `MyClass::method`)
    pub symbol: String,

    /// Operation: "replace" (swap entire definition), "insert_after", "insert_before"
    pub operation: String,

    /// New code/text content for the operation
    pub content: String,

    /// Disambiguate when multiple symbols share a name (partial file path match)
    #[serde(default)]
    pub file_path: Option<String>,

    /// Preview diff without applying (default: true). Always preview first.
    #[serde(
        default = "default_dry_run",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub dry_run: bool,
}

/// Detect the line ending used in source content.
fn detect_line_ending(content: &str) -> &'static str {
    if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Replace lines start_line..=end_line (1-indexed, inclusive) with new content.
pub fn replace_symbol_body(
    source: &str,
    start_line: u32,
    end_line: u32,
    new_content: &str,
) -> Result<String> {
    let eol = detect_line_ending(source);
    let lines: Vec<&str> = source.lines().collect();
    let start_idx = (start_line as usize).saturating_sub(1); // 1-indexed to 0-indexed
    let end_idx = end_line as usize; // 1-indexed end_line; used as exclusive bound

    if start_idx >= lines.len() || end_idx > lines.len() {
        return Err(anyhow!(
            "Line range {}-{} is outside file bounds (file has {} lines)",
            start_line,
            end_line,
            lines.len()
        ));
    }

    let mut result = String::new();
    // Lines before the symbol
    for line in &lines[..start_idx] {
        result.push_str(line);
        result.push_str(eol);
    }
    // New content replacing the symbol
    result.push_str(new_content);
    if !new_content.ends_with('\n') && !new_content.ends_with("\r\n") {
        result.push_str(eol);
    }
    // Lines after the symbol
    for line in &lines[end_idx..] {
        result.push_str(line);
        result.push_str(eol);
    }

    // Preserve original trailing newline behavior
    if !source.ends_with('\n') && result.ends_with('\n') {
        result.pop();
        if eol == "\r\n" && result.ends_with('\r') {
            result.pop();
        }
    }

    Ok(result)
}

/// Insert content before or after a specific line (1-indexed).
/// "after" inserts on a new line after anchor_line.
/// "before" inserts on a new line before anchor_line.
pub fn insert_near_symbol(
    source: &str,
    anchor_line: u32,
    new_content: &str,
    position: &str,
) -> Result<String> {
    let eol = detect_line_ending(source);
    let lines: Vec<&str> = source.lines().collect();
    let anchor_idx = (anchor_line as usize).saturating_sub(1); // 1-indexed to 0-indexed

    if anchor_idx >= lines.len() {
        return Err(anyhow!(
            "Line {} is outside file bounds (file has {} lines)",
            anchor_line,
            lines.len()
        ));
    }

    let mut result = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == anchor_idx && position == "before" {
            result.push_str(new_content);
            if !new_content.ends_with('\n') && !new_content.ends_with("\r\n") {
                result.push_str(eol);
            }
        }
        result.push_str(line);
        result.push_str(eol);
        if i == anchor_idx && position == "after" {
            result.push_str(new_content);
            if !new_content.ends_with('\n') && !new_content.ends_with("\r\n") {
                result.push_str(eol);
            }
        }
    }

    Ok(result)
}

impl EditSymbolTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        // Validate parameters
        if self.symbol.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: symbol name is required".to_string(),
            )]));
        }
        if !["replace", "insert_after", "insert_before"].contains(&self.operation.as_str()) {
            return Ok(CallToolResult::text_content(vec![Content::text(format!(
                "Error: operation must be 'replace', 'insert_after', or 'insert_before', got '{}'",
                self.operation
            ))]));
        }

        // Get workspace and database
        let workspace = handler.get_workspace().await?.ok_or_else(|| {
            anyhow!("No workspace initialized. Run manage_workspace(operation=\"index\") first.")
        })?;
        let db_arc = workspace
            .db
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "Database not available. Run manage_workspace(operation=\"index\") first."
                )
            })?
            .clone();

        // Look up symbol using deep_dive's find_symbol (handles qualified names,
        // flat namespaces, parent/child resolution)
        let symbol_name = self.symbol.clone();
        let file_path_filter = self.file_path.clone();
        let file_path_for_error = self.file_path.clone();
        let matches = tokio::task::spawn_blocking(move || -> Result<Vec<(String, String, u32, u32)>> {
            let db = db_arc
                .lock()
                .map_err(|e| anyhow!("Database lock error: {}", e))?;
            let symbols = crate::tools::deep_dive::data::find_symbol(
                &db,
                &symbol_name,
                file_path_filter.as_deref(),
            )?;
            // find_symbol falls back to unfiltered results when the file filter
            // matches nothing. That's fine for read-only deep_dive, but for a write
            // operation we must enforce the filter strictly.
            let filtered = if let Some(ref fp) = file_path_filter {
                symbols
                    .into_iter()
                    .filter(|s| s.file_path.contains(fp.as_str()))
                    .collect()
            } else {
                symbols
            };
            Ok(filtered
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        s.file_path.clone(),
                        s.start_line,
                        s.end_line,
                    )
                })
                .collect())
        })
        .await??;

        if matches.is_empty() {
            if let Some(ref fp) = file_path_for_error {
                return Ok(CallToolResult::text_content(vec![Content::text(format!(
                    "Error: symbol '{}' not found in '{}'. The symbol may exist in other files. \
                     Use fast_search or get_symbols to verify the location.",
                    self.symbol, fp
                ))]));
            }
            return Ok(CallToolResult::text_content(vec![Content::text(format!(
                "Error: symbol '{}' not found in index. Use fast_search or get_symbols to verify the name.",
                self.symbol
            ))]));
        }

        if matches.len() > 1 {
            let locations: Vec<String> = matches
                .iter()
                .map(|(name, path, start, end)| {
                    format!("  {} at {}:{}-{}", name, path, start, end)
                })
                .collect();
            return Ok(CallToolResult::text_content(vec![Content::text(format!(
                "Error: '{}' matches {} symbols. Provide file_path to disambiguate:\n{}",
                self.symbol,
                matches.len(),
                locations.join("\n")
            ))]));
        }

        let (_, symbol_file, start_line, end_line) = &matches[0];

        // Resolve the file path (security check)
        let workspace_root = &handler.workspace_root;
        let resolved_path = secure_path_resolution(symbol_file, workspace_root)?;
        let resolved_str = resolved_path.to_string_lossy().to_string();

        // Read file content internally
        let original_content = std::fs::read_to_string(&resolved_path)
            .map_err(|e| anyhow!("Cannot read file '{}': {}", symbol_file, e))?;

        // Apply the operation
        let modified_content = match self.operation.as_str() {
            "replace" => {
                replace_symbol_body(&original_content, *start_line, *end_line, &self.content)?
            }
            "insert_after" => {
                insert_near_symbol(&original_content, *end_line, &self.content, "after")?
            }
            "insert_before" => {
                insert_near_symbol(&original_content, *start_line, &self.content, "before")?
            }
            _ => unreachable!(),
        };

        // Balance validation for code files
        if should_check_balance(symbol_file) {
            if let Err(e) = check_bracket_balance(&original_content, &modified_content) {
                return Ok(CallToolResult::text_content(vec![Content::text(format!(
                    "Edit rejected: {}. Review the content and try again.",
                    e
                ))]));
            }
        }

        // Generate diff
        let diff = format_unified_diff(&original_content, &modified_content, symbol_file);

        if self.dry_run {
            debug!(
                "edit_symbol dry_run for {} in {}",
                self.symbol, symbol_file
            );
            return Ok(CallToolResult::text_content(vec![Content::text(format!(
                "Dry run preview (set dry_run=false to apply):\n\n{}",
                diff
            ))]));
        }

        // Commit atomically
        let txn = EditingTransaction::begin(&resolved_str)?;
        txn.commit(&modified_content)?;

        debug!(
            "edit_symbol {} applied to {}",
            self.operation, symbol_file
        );
        Ok(CallToolResult::text_content(vec![Content::text(format!(
            "Applied {} on '{}' in {}:\n\n{}",
            self.operation, self.symbol, symbol_file, diff
        ))]))
    }
}
