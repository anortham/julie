//! edit_file tool: DMP-powered fuzzy find-and-replace.
//!
//! Lets agents edit files without reading them first. The agent provides
//! old_text (what to find) and new_text (what to replace with). DMP's
//! fuzzy matching tolerates minor differences.

use anyhow::{anyhow, Result};
use diff_match_patch_rs::{Compat, DiffMatchPatch};
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

fn default_occurrence() -> String {
    "first".to_string()
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct EditFileTool {
    /// File path relative to workspace root
    pub file_path: String,

    /// Text to find in the file (fuzzy-matched via diff-match-patch)
    pub old_text: String,

    /// Replacement text
    pub new_text: String,

    /// Preview diff without applying (default: true). Always preview first.
    #[serde(
        default = "default_dry_run",
        deserialize_with = "crate::utils::serde_lenient::deserialize_bool_lenient"
    )]
    pub dry_run: bool,

    /// Which occurrence to replace: "first" (default), "last", or "all"
    #[serde(default = "default_occurrence")]
    pub occurrence: String,
}

/// Pure function: apply an edit to content string. Returns modified content.
/// Separated from tool struct for testability.
pub fn apply_edit(
    content: &str,
    old_text: &str,
    new_text: &str,
    occurrence: &str,
) -> Result<String> {
    if old_text.is_empty() {
        return Err(anyhow!("old_text cannot be empty"));
    }

    let positions = find_all_matches(content, old_text)?;

    if positions.is_empty() {
        return Err(anyhow!(
            "No match found for the provided old_text ({} chars). \
             Verify the text exists in the file.",
            old_text.len()
        ));
    }

    let selected: Vec<usize> = match occurrence {
        "first" => vec![positions[0]],
        "last" => vec![*positions.last().unwrap()],
        "all" => positions,
        _ => {
            return Err(anyhow!(
                "Invalid occurrence '{}': must be 'first', 'last', or 'all'",
                occurrence
            ))
        }
    };

    // Apply replacements in reverse order so character positions don't shift
    let old_char_len = old_text.chars().count();
    let new_chars: Vec<char> = new_text.chars().collect();
    let mut result_chars: Vec<char> = content.chars().collect();

    for &pos in selected.iter().rev() {
        result_chars.splice(pos..pos + old_char_len, new_chars.iter().copied());
    }

    Ok(result_chars.into_iter().collect())
}

/// Find all match positions for old_text in content.
///
/// Strategy:
/// 1. Exact substring search (works for any pattern length, always tried first)
/// 2. If no exact matches and pattern fits within DMP's bitap limit (32 chars),
///    fall back to DMP fuzzy matching (tolerates minor whitespace/typo differences)
///
/// Returns character positions sorted ascending.
fn find_all_matches(content: &str, old_text: &str) -> Result<Vec<usize>> {
    let old_char_len = old_text.chars().count();

    // Phase 1: try exact substring search (handles any pattern length)
    let exact_positions = find_exact_matches(content, old_text);
    if !exact_positions.is_empty() {
        return Ok(exact_positions);
    }

    // Phase 2: DMP fuzzy fallback for short patterns only (bitap limit is 32 chars)
    // This helps when the agent has minor whitespace/formatting differences.
    const DMP_BITAP_LIMIT: usize = 32;
    if old_char_len > DMP_BITAP_LIMIT {
        // Pattern too long for DMP bitap; exact search was authoritative
        return Ok(Vec::new());
    }

    let dmp = DiffMatchPatch::new();
    let mut positions = Vec::new();
    let mut search_from: usize = 0;

    loop {
        match dmp.match_main::<Compat>(content, old_text, search_from) {
            Some(pos) if pos >= search_from => {
                positions.push(pos);
                search_from = pos + old_char_len;
            }
            _ => break,
        }
    }

    Ok(positions)
}

/// Exact substring search returning all character-index positions ascending.
fn find_exact_matches(content: &str, pattern: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut byte_start = 0;

    while let Some(byte_pos) = content[byte_start..].find(pattern) {
        let abs_byte_pos = byte_start + byte_pos;
        // Convert byte position to char position
        let char_pos = content[..abs_byte_pos].chars().count();
        positions.push(char_pos);
        // Advance past this match (at least one char to avoid infinite loops on empty patterns)
        byte_start = abs_byte_pos + pattern.len().max(1);
    }

    positions
}

impl EditFileTool {
    pub async fn call_tool(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
        if self.old_text.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: old_text is required and cannot be empty".to_string(),
            )]));
        }

        // Resolve and validate file path (security check)
        let workspace_root = &handler.workspace_root;
        let resolved_path = secure_path_resolution(&self.file_path, workspace_root)?;
        let resolved_str = resolved_path.to_string_lossy().to_string();

        // Read file content internally (not costing agent context tokens)
        let original_content = std::fs::read_to_string(&resolved_path)
            .map_err(|e| anyhow!("Cannot read file '{}': {}", self.file_path, e))?;

        // Apply the edit
        let modified_content = match apply_edit(
            &original_content,
            &self.old_text,
            &self.new_text,
            &self.occurrence,
        ) {
            Ok(content) => content,
            Err(e) => {
                return Ok(CallToolResult::text_content(vec![Content::text(
                    format!("Error: {}", e),
                )]));
            }
        };

        // Balance validation for code files
        if should_check_balance(&self.file_path) {
            if let Err(e) = check_bracket_balance(&original_content, &modified_content) {
                return Ok(CallToolResult::text_content(vec![Content::text(
                    format!(
                        "Edit rejected: {}. The edit would create unbalanced brackets. \
                         Review old_text/new_text and try again.",
                        e
                    ),
                )]));
            }
        }

        // Generate diff preview
        let diff = format_unified_diff(&original_content, &modified_content, &self.file_path);

        if self.dry_run {
            debug!("edit_file dry_run for {}", self.file_path);
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!("Dry run preview (set dry_run=false to apply):\n\n{}", diff),
            )]));
        }

        // Commit the edit atomically
        let txn = EditingTransaction::begin(&resolved_str)?;
        txn.commit(&modified_content)?;

        debug!("edit_file applied to {}", self.file_path);
        Ok(CallToolResult::text_content(vec![Content::text(
            format!("Applied edit to {}:\n\n{}", self.file_path, diff),
        )]))
    }
}
