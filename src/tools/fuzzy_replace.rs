//! Fuzzy Replace Tool - DMP-powered fuzzy text matching and replacement
//!
//! This tool uses diff-match-patch's fuzzy matching capabilities to find and replace
//! "similar" text even when it doesn't match exactly. This is Google's battle-tested
//! algorithm used in Google Docs since 2006.
//!
//! Use cases:
//! - Fixing typos across similar code patterns
//! - Updating code that has slight variations
//! - Refactoring when exact matches are impractical
//!
//! DMP Fuzzy Matching Features:
//! - Match_Threshold: How similar text needs to be (0.0 = perfect, 1.0 = anything)
//! - Match_Distance: How far away to search for matches
//! - match_main: Returns best match location even for imperfect matches

use anyhow::{anyhow, Result};
use diff_match_patch_rs::{DiffMatchPatch, Efficient};
use rust_mcp_sdk::macros::mcp_tool;
use rust_mcp_sdk::macros::JsonSchema;
use rust_mcp_sdk::schema::{CallToolResult, TextContent};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{debug, info};

use crate::utils::token_estimation::TokenEstimator;

/// Structured result from fuzzy replace operation
#[derive(Debug, Clone, Serialize)]
pub struct FuzzyReplaceResult {
    pub tool: String,
    pub file_path: String,
    pub pattern: String,
    pub replacement: String,
    pub matches_found: usize,
    pub threshold: f32,
    pub dry_run: bool,
    pub validation_passed: bool,
    pub next_actions: Vec<String>,
}

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

        // Dry run - show preview with structured output
        if self.dry_run {
            // Create structured result
            let result = FuzzyReplaceResult {
                tool: "fuzzy_replace".to_string(),
                file_path: self.file_path.clone(),
                pattern: self.pattern.clone(),
                replacement: self.replacement.clone(),
                matches_found,
                threshold: self.threshold,
                dry_run: true,
                validation_passed: true,
                next_actions: vec![
                    "Review the changes preview above".to_string(),
                    format!("Set dry_run=false to apply {} replacements", matches_found),
                ],
            };

            // Format human-readable markdown
            let mut markdown = String::new();
            markdown.push_str(&format!("📋 **Fuzzy Replace Preview: {}**\n\n", result.file_path));
            markdown.push_str(&format!("**Matches found:** {}\n", result.matches_found));
            markdown.push_str(&format!("**Pattern:** `{}`\n", result.pattern));
            markdown.push_str(&format!("**Replacement:** `{}`\n", result.replacement));
            markdown.push_str(&format!("**Threshold:** {} (fuzzy matching)\n", result.threshold));
            markdown.push_str(&format!("**Distance:** {} characters\n\n", self.distance));

            // Show diff preview (simplified - just show summary)
            markdown.push_str("**Changes Preview:**\n");
            let lines_changed = modified_content.lines().count().abs_diff(original_content.lines().count());
            markdown.push_str(&format!("• Lines changed: ~{}\n", lines_changed));
            markdown.push_str(&format!("• Original length: {} chars\n", original_content.len()));
            markdown.push_str(&format!("• Modified length: {} chars\n\n", modified_content.len()));
            markdown.push_str("💡 Set dry_run: false to apply changes\n");

            // Serialize to JSON
            let structured = serde_json::to_value(&result)
                .map_err(|e| anyhow!("Failed to serialize result: {}", e))?;

            let structured_map = if let serde_json::Value::Object(map) = structured {
                map
            } else {
                return Err(anyhow!("Expected JSON object"));
            };

            return Ok(CallToolResult::text_content(vec![TextContent::from(
                self.optimize_response(&markdown),
            )])
            .with_structured_content(structured_map));
        }

        // Apply changes atomically using transaction
        let temp_file = format!("{}.fuzzy_tmp", self.file_path);
        fs::write(&temp_file, &modified_content)
            .map_err(|e| anyhow!("Failed to write temp file: {}", e))?;

        fs::rename(&temp_file, &self.file_path)
            .map_err(|e| anyhow!("Failed to apply changes: {}", e))?;

        // Create structured result
        let result = FuzzyReplaceResult {
            tool: "fuzzy_replace".to_string(),
            file_path: self.file_path.clone(),
            pattern: self.pattern.clone(),
            replacement: self.replacement.clone(),
            matches_found,
            threshold: self.threshold,
            dry_run: false,
            validation_passed: true,
            next_actions: vec![
                format!("Review changes in: {}", self.file_path),
                "Run tests to verify functionality".to_string(),
            ],
        };

        // Format human-readable markdown
        let markdown = format!(
            "✅ **Fuzzy Replace Complete: {}**\n\n\
             **Matches replaced:** {}\n\
             **Pattern:** `{}`\n\
             **Replacement:** `{}`\n\
             **Threshold:** {} (fuzzy matching)\n\n\
             Changes applied successfully!",
            result.file_path, result.matches_found, result.pattern,
            result.replacement, result.threshold
        );

        // Serialize to JSON for structured_content
        let structured = serde_json::to_value(&result)
            .map_err(|e| anyhow!("Failed to serialize result: {}", e))?;

        let structured_map = if let serde_json::Value::Object(map) = structured {
            map
        } else {
            return Err(anyhow!("Expected JSON object"));
        };

        Ok(CallToolResult::text_content(vec![TextContent::from(markdown)])
            .with_structured_content(structured_map))
    }

    /// Perform fuzzy search and replace using hybrid DMP + Levenshtein approach
    ///
    /// **Strategy:** Use Google's DMP for fast candidate finding, then validate with Levenshtein
    /// - DMP's bitap algorithm quickly finds potential matches (even with errors)
    /// - Levenshtein similarity provides precise quality filtering
    /// - This combines DMP's speed with our accuracy requirements
    ///
    /// Collects all matches first, then applies replacements in reverse to maintain indices
    pub(crate) fn fuzzy_search_replace(&self, content: &str) -> Result<(String, usize)> {
        if self.pattern.is_empty() {
            return Ok((content.to_string(), 0));
        }

        // Configure DMP for candidate finding
        // Note: DMP's threshold is used for its internal scoring, but we validate separately
        let mut dmp = DiffMatchPatch::new();
        dmp.set_match_threshold(self.threshold);
        dmp.set_match_distance(self.distance as usize);

        // Convert to char-based for consistent indexing
        let content_chars: Vec<char> = content.chars().collect();
        let pattern_chars: Vec<char> = self.pattern.chars().collect();
        let pattern_len = pattern_chars.len();

        // If pattern is longer than content, no matches possible
        if pattern_len > content_chars.len() {
            return Ok((content.to_string(), 0));
        }

        // Find all matches using DMP's match_main
        let mut matches: Vec<usize> = Vec::new();
        let mut search_from_char = 0;

        while search_from_char + pattern_len <= content_chars.len() {
            // Convert char slice to string for DMP
            let search_content: String = content_chars[search_from_char..].iter().collect();

            // Use DMP's fuzzy matching to find next match at the START of search_content
            // loc=0 means we expect the match near the beginning
            match dmp.match_main::<Efficient>(&search_content, &self.pattern, 0) {
                Some(byte_offset) => {
                    // DMP returns byte offset, convert to char offset in search_content
                    let char_offset_in_slice = search_content[..byte_offset]
                        .chars()
                        .count();

                    // Convert to absolute char position in original content
                    let absolute_char_pos = search_from_char + char_offset_in_slice;

                    // Validate match is within bounds
                    if absolute_char_pos + pattern_len <= content_chars.len() {
                        // Extract the actual matched text
                        let matched_text: String = content_chars[absolute_char_pos..absolute_char_pos + pattern_len]
                            .iter()
                            .collect();

                        // Verify similarity meets our threshold
                        // DMP's threshold is for finding candidates, but we need to validate quality
                        let similarity = self.calculate_similarity(&self.pattern, &matched_text);

                        if similarity >= self.threshold {
                            debug!(
                                "DMP fuzzy match at char {} (similarity: {:.2}): '{}'",
                                absolute_char_pos, similarity, matched_text
                            );

                            matches.push(absolute_char_pos);

                            // Continue searching after this match to avoid overlaps
                            search_from_char = absolute_char_pos + pattern_len;
                        } else {
                            debug!(
                                "DMP match rejected (similarity {:.2} < threshold {}): '{}'",
                                similarity, self.threshold, matched_text
                            );
                            // DMP found something, but it's not good enough - advance by 1 char and keep looking
                            search_from_char = absolute_char_pos + 1;
                        }
                    } else {
                        // Match would exceed bounds, stop searching
                        break;
                    }
                }
                None => {
                    // No more matches found in this region
                    break;
                }
            }
        }

        // Apply replacements in reverse order to maintain indices
        let mut result_chars = content_chars;
        let replacement_chars: Vec<char> = self.replacement.chars().collect();

        for &match_pos in matches.iter().rev() {
            // Double-check bounds before splicing
            if match_pos + pattern_len <= result_chars.len() {
                result_chars.splice(match_pos..match_pos + pattern_len, replacement_chars.clone());
            }
        }

        Ok((result_chars.iter().collect(), matches.len()))
    }

    /// Calculate similarity between two strings (0.0 = completely different, 1.0 = identical)
    /// Uses Levenshtein distance calculation
    ///
    /// NOTE: This method is kept for backward compatibility with existing tests.
    /// The actual fuzzy matching now uses DMP's match_main algorithm.
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
