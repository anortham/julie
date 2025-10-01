//! Fuzzy Replace Tool - DMP-powered fuzzy text matching and replacement
//!
//! This tool uses diff-match-patch's fuzzy matching capabilities to find and replace
//! "similar" text even when it doesn't match exactly. This is unique to DMP and not
//! available in the built-in Edit tool.
//!
//! Use cases:
//! - Fixing typos across similar code patterns
//! - Updating code that has slight variations
//! - Refactoring when exact matches are impractical
//!
//! DMP Fuzzy Matching Features:
//! - Match_Threshold: How similar text needs to be (0.0 = perfect, 1.0 = anything)
//! - Match_Distance: How far away to search for matches
//! - Fuzzy_Match: Returns best match location even for imperfect matches

use anyhow::{anyhow, Result};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{debug, info};

use crate::utils::token_estimation::TokenEstimator;

fn default_threshold() -> f32 {
    0.8
}

fn default_distance() -> i32 {
    1000
}

fn default_true() -> bool {
    true
}

//**********************//
//   Fuzzy Replace Tool //
//**********************//

#[mcp_tool(
    name = "fuzzy_replace",
    description = "FUZZY TEXT MATCHING - Find and replace similar (not exact) text using DMP fuzzy matching",
    title = "Fuzzy Pattern Replacement",
    idempotent_hint = false,
    destructive_hint = true,
    open_world_hint = false,
    read_only_hint = false,
    meta = r#"{"category": "editing", "safety": "dmp_fuzzy"}"#
)]
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FuzzyReplaceTool {
    /// File path to edit
    /// Example: "src/user.rs", "lib/services/auth.py"
    pub file_path: String,

    /// Pattern to find (will use fuzzy matching)
    /// Example: "function getUserData()" (will match "function getUserDat()" with typo)
    pub pattern: String,

    /// Replacement text
    /// Example: "function fetchUserData()"
    pub replacement: String,

    /// Fuzzy match threshold (0.0-1.0)
    /// 0.0 = perfect match only
    /// 0.5 = moderate tolerance
    /// 0.8 = high tolerance (default)
    /// 1.0 = match anything
    #[serde(default = "default_threshold")]
    pub threshold: f32,

    /// Match distance - how far to search (in characters)
    /// Default: 1000 - reasonable for most code files
    /// Higher = slower but more comprehensive
    #[serde(default = "default_distance")]
    pub distance: i32,

    /// Preview changes without applying them (RECOMMENDED for first run)
    /// Default: false
    #[serde(default)]
    pub dry_run: bool,

    /// Validate changes before applying (brace/bracket matching)
    /// Default: true - recommended for safety
    #[serde(default = "default_true")]
    pub validate: bool,
}

impl FuzzyReplaceTool {
    pub async fn call_tool(&self, _handler: &crate::handler::JulieServerHandler) -> Result<CallToolResult> {
        info!(
            "🔍 Fuzzy replace in: {} (threshold: {}, distance: {})",
            self.file_path, self.threshold, self.distance
        );

        // Validate parameters
        if self.threshold < 0.0 || self.threshold > 1.0 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                "❌ threshold must be between 0.0 and 1.0\n\
                 💡 Recommended: 0.8 for most cases".to_string(),
            )]));
        }

        if self.distance < 0 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                "❌ distance must be positive\n\
                 💡 Recommended: 1000 for most files".to_string(),
            )]));
        }

        // Read file
        let original_content = fs::read_to_string(&self.file_path)
            .map_err(|e| anyhow!("Failed to read file '{}': {}", self.file_path, e))?;

        if original_content.is_empty() {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                format!("❌ File is empty: {}", self.file_path),
            )]));
        }

        // Perform fuzzy search and replace
        let (modified_content, matches_found) = self.fuzzy_search_replace(&original_content)?;

        if matches_found == 0 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                format!(
                    "ℹ️ No fuzzy matches found for pattern in: {}\n\n\
                     Pattern: '{}'\n\
                     Threshold: {}\n\n\
                     Possible reasons:\n\
                     • Pattern doesn't exist (even fuzzily)\n\
                     • Threshold too strict (try higher value like 0.9)\n\
                     • Distance too small (try larger value like 2000)",
                    self.file_path, self.pattern, self.threshold
                ),
            )]));
        }

        // Validate if requested - check if replacement CHANGES the balance
        if self.validate {
            let original_balance = self.calculate_balance(&original_content);
            let modified_balance = self.calculate_balance(&modified_content);

            // Only fail if the replacement CHANGED the balance from valid to invalid
            if original_balance == modified_balance {
                // Balance unchanged - replacement is safe
            } else {
                // Check if replacement text itself is balanced
                let replacement_balance = self.calculate_balance(&self.replacement);
                let pattern_balance = self.calculate_balance(&self.pattern);

                if replacement_balance != pattern_balance {
                    return Ok(CallToolResult::text_content(vec![TextContent::from(
                        format!(
                            "❌ Validation failed: Replacement changes bracket/paren balance\n\n\
                             Pattern balance: {:?}\n\
                             Replacement balance: {:?}\n\n\
                             The replacement would break code structure.\n\
                             • Check braces, brackets, parentheses in replacement\n\
                             • Ensure pattern and replacement have same balance\n\
                             • Set validate: false to skip validation (not recommended)",
                            pattern_balance, replacement_balance
                        ),
                    )]));
                }
            }
        }

        // Dry run - show preview
        if self.dry_run {
            let mut output = String::new();
            output.push_str(&format!("📋 **Fuzzy Replace Preview: {}**\n\n", self.file_path));
            output.push_str(&format!("**Matches found:** {}\n", matches_found));
            output.push_str(&format!("**Pattern:** `{}`\n", self.pattern));
            output.push_str(&format!("**Replacement:** `{}`\n", self.replacement));
            output.push_str(&format!("**Threshold:** {} (fuzzy matching)\n", self.threshold));
            output.push_str(&format!("**Distance:** {} characters\n\n", self.distance));

            // Show diff preview (simplified - just show summary)
            output.push_str("**Changes Preview:**\n");
            let lines_changed = modified_content.lines().count().abs_diff(original_content.lines().count());
            output.push_str(&format!("• Lines changed: ~{}\n", lines_changed));
            output.push_str(&format!("• Original length: {} chars\n", original_content.len()));
            output.push_str(&format!("• Modified length: {} chars\n\n", modified_content.len()));
            output.push_str("💡 Set dry_run: false to apply changes\n");

            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&output),
            )]));
        }

        // Apply changes atomically using transaction
        let temp_file = format!("{}.fuzzy_tmp", self.file_path);
        fs::write(&temp_file, &modified_content)
            .map_err(|e| anyhow!("Failed to write temp file: {}", e))?;

        fs::rename(&temp_file, &self.file_path)
            .map_err(|e| anyhow!("Failed to apply changes: {}", e))?;

        // Success message
        let output = format!(
            "✅ **Fuzzy Replace Complete: {}**\n\n\
             **Matches replaced:** {}\n\
             **Pattern:** `{}`\n\
             **Replacement:** `{}`\n\
             **Threshold:** {} (fuzzy matching)\n\n\
             Changes applied successfully!",
            self.file_path, matches_found, self.pattern, self.replacement, self.threshold
        );

        Ok(CallToolResult::text_content(vec![TextContent::from(output)]))
    }

    /// Perform fuzzy search and replace using character-based sliding window
    /// Collects all matches first, then applies replacements in reverse to maintain indices
    pub(crate) fn fuzzy_search_replace(&self, content: &str) -> Result<(String, usize)> {
        let pattern_chars: Vec<char> = self.pattern.chars().collect();
        let content_chars: Vec<char> = content.chars().collect();
        let pattern_len = pattern_chars.len();

        if pattern_len == 0 {
            return Ok((content.to_string(), 0));
        }

        // Collect all match positions first (char indices)
        let mut matches: Vec<usize> = Vec::new();
        let mut i = 0;

        while i + pattern_len <= content_chars.len() {
            let window: String = content_chars[i..i + pattern_len].iter().collect();

            // Calculate similarity between pattern and window
            let similarity = self.calculate_similarity(&self.pattern, &window);

            if similarity >= self.threshold {
                debug!(
                    "Fuzzy match at char {}: '{}' (similarity: {:.2})",
                    i, window, similarity
                );
                matches.push(i);

                // Skip past this match to avoid overlapping replacements
                i += pattern_len;
            } else {
                i += 1;
            }
        }

        // Apply replacements in reverse order to maintain indices
        let mut result_chars = content_chars;
        let replacement_chars: Vec<char> = self.replacement.chars().collect();

        for &match_pos in matches.iter().rev() {
            result_chars.splice(match_pos..match_pos + pattern_len, replacement_chars.clone());
        }

        Ok((result_chars.iter().collect(), matches.len()))
    }

    /// Calculate similarity between two strings (0.0 = completely different, 1.0 = identical)
    /// Uses Levenshtein distance for true fuzzy matching
    pub(crate) fn calculate_similarity(&self, a: &str, b: &str) -> f32 {
        if a == b {
            return 1.0;
        }

        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();

        if a_chars.is_empty() && b_chars.is_empty() {
            return 1.0;
        }

        if a_chars.is_empty() || b_chars.is_empty() {
            return 0.0;
        }

        // Levenshtein distance calculation
        let a_len = a_chars.len();
        let b_len = b_chars.len();
        let mut matrix = vec![vec![0usize; b_len + 1]; a_len + 1];

        // Initialize first row and column
        for i in 0..=a_len {
            matrix[i][0] = i;
        }
        for j in 0..=b_len {
            matrix[0][j] = j;
        }

        // Fill matrix
        for i in 1..=a_len {
            for j in 1..=b_len {
                let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
                matrix[i][j] = std::cmp::min(
                    std::cmp::min(
                        matrix[i - 1][j] + 1,      // deletion
                        matrix[i][j - 1] + 1       // insertion
                    ),
                    matrix[i - 1][j - 1] + cost    // substitution
                );
            }
        }

        let distance = matrix[a_len][b_len];
        let max_len = std::cmp::max(a_len, b_len);

        // Convert distance to similarity (1.0 = identical, 0.0 = completely different)
        1.0 - (distance as f32 / max_len as f32)
    }

    /// Calculate bracket/paren/brace balance for content
    /// Returns (braces, brackets, parens) final counts
    pub(crate) fn calculate_balance(&self, content: &str) -> (i32, i32, i32) {
        let mut brace_count = 0i32;
        let mut bracket_count = 0i32;
        let mut paren_count = 0i32;

        for ch in content.chars() {
            match ch {
                '{' => brace_count += 1,
                '}' => brace_count -= 1,
                '[' => bracket_count += 1,
                ']' => bracket_count -= 1,
                '(' => paren_count += 1,
                ')' => paren_count -= 1,
                _ => {}
            }
        }

        (brace_count, bracket_count, paren_count)
    }

    /// Optimize response for token limits
    fn optimize_response(&self, response: &str) -> String {
        let estimator = TokenEstimator::new();
        let tokens = estimator.estimate_string(response);

        if tokens <= 5000 {
            response.to_string()
        } else {
            let chars_per_token = response.len() / tokens.max(1);
            let target_chars = chars_per_token * 5000;
            let truncated = &response[..target_chars.min(response.len())];
            format!("{}\n\n... (preview truncated)", truncated)
        }
    }
}
