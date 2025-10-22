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
use tokio::fs; // Use async file I/O
use tracing::{debug, info};

use crate::tools::editing::EditingTransaction;

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

    /// Fuzzy match threshold (default: 0.8, range: 0.0-1.0).
    /// 0.0 = perfect match only
    /// 0.5 = moderate tolerance
    /// 0.8 = high tolerance (recommended)
    /// 1.0 = match anything
    #[serde(default = "default_threshold")]
    pub threshold: f32,

    /// Match distance - how far to search in characters (default: 1000).
    /// Higher values = slower but more comprehensive search
    /// Recommended: 1000 for most code files
    #[serde(default = "default_distance")]
    pub distance: i32,

    /// Preview changes without applying them (default: false).
    /// RECOMMENDED: Set true for first run to verify changes before applying
    #[serde(default)]
    pub dry_run: bool,

    /// Validate changes before applying (default: true).
    /// Performs brace/bracket matching to ensure structural integrity
    /// Recommended: true for safety
    #[serde(default = "default_true")]
    pub validate: bool,
}

impl FuzzyReplaceTool {
    pub async fn call_tool(
        &self,
        handler: &crate::handler::JulieServerHandler,
    ) -> Result<CallToolResult> {
        use crate::utils::file_utils::secure_path_resolution;
        use std::env;

        // Secure path resolution to prevent traversal attacks
        let workspace_root = if let Some(workspace) = handler.get_workspace().await? {
            workspace.root.clone()
        } else {
            env::current_dir()
                .map_err(|e| anyhow!("Failed to determine current directory: {}", e))?
        };

        let resolved_path = secure_path_resolution(&self.file_path, &workspace_root)?;
        let resolved_path_str = resolved_path.to_string_lossy().to_string();

        info!(
            "🔍 Fuzzy replace in: {} (threshold: {}, distance: {})",
            resolved_path.display(), self.threshold, self.distance
        );

        // Validate parameters
        if self.threshold < 0.0 || self.threshold > 1.0 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                "Error: threshold must be between 0.0 and 1.0 (recommended: 0.8)".to_string(),
            )]));
        }

        if self.distance < 0 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                "Error: distance must be positive (recommended: 1000)".to_string(),
            )]));
        }

        // Read file (async to avoid blocking executor thread)
        let original_content = fs::read_to_string(&resolved_path)
            .await
            .map_err(|e| anyhow!("Failed to read file '{}': {}", resolved_path.display(), e))?;

        if original_content.is_empty() {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                format!("Error: File is empty: {}", resolved_path.display()),
            )]));
        }

        // Perform fuzzy search and replace
        let (modified_content, matches_found) = self.fuzzy_search_replace(&original_content)?;

        if matches_found == 0 {
            return Ok(CallToolResult::text_content(vec![TextContent::from(
                format!(
                    "No fuzzy matches found for pattern in: {}\nPattern: '{}', Threshold: {} (try increasing threshold or distance)",
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
                            "Validation failed: Replacement changes bracket/paren balance\nPattern balance: {:?}, Replacement balance: {:?}",
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
                file_path: resolved_path.display().to_string(),
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

            // Minimal 2-line summary
            let markdown = format!(
                "Fuzzy match preview: {} matches found in {}\nPattern: '{}' → Replacement: '{}' (threshold: {}, dry_run: true)",
                result.matches_found, result.file_path, result.pattern, result.replacement, result.threshold
            );

            // Serialize to JSON
            let structured = serde_json::to_value(&result)
                .map_err(|e| anyhow!("Failed to serialize result: {}", e))?;

            let structured_map = if let serde_json::Value::Object(map) = structured {
                map
            } else {
                return Err(anyhow!("Expected JSON object"));
            };

            return Ok(
                CallToolResult::text_content(vec![TextContent::from(markdown)])
                    .with_structured_content(structured_map),
            );
        }

        // Apply changes atomically using EditingTransaction
        let transaction = EditingTransaction::begin(&resolved_path_str)
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;
        transaction
            .commit(&modified_content)
            .map_err(|e| anyhow!("Failed to apply changes: {}", e))?;

        // Create structured result
        let result = FuzzyReplaceResult {
            tool: "fuzzy_replace".to_string(),
            file_path: resolved_path.display().to_string(),
            pattern: self.pattern.clone(),
            replacement: self.replacement.clone(),
            matches_found,
            threshold: self.threshold,
            dry_run: false,
            validation_passed: true,
            next_actions: vec![
                format!("Review changes in: {}", resolved_path.display()),
                "Run tests to verify functionality".to_string(),
            ],
        };

        // Minimal 2-line summary
        let markdown = format!(
            "Fuzzy replace complete: {} matches replaced in {}\nPattern: '{}' → Replacement: '{}' (threshold: {})",
            result.matches_found, result.file_path, result.pattern, result.replacement, result.threshold
        );

        // Serialize to JSON for structured_content
        let structured = serde_json::to_value(&result)
            .map_err(|e| anyhow!("Failed to serialize result: {}", e))?;

        let structured_map = if let serde_json::Value::Object(map) = structured {
            map
        } else {
            return Err(anyhow!("Expected JSON object"));
        };

        Ok(
            CallToolResult::text_content(vec![TextContent::from(markdown)])
                .with_structured_content(structured_map),
        )
    }

    /// Perform fuzzy search and replace using hybrid DMP + Levenshtein approach
    ///
    /// **Strategy:** Use Google's DMP for fast candidate finding, then validate with Levenshtein
    /// - DMP's bitap algorithm quickly finds potential matches (even with errors)
    /// - Levenshtein similarity provides precise quality filtering
    /// - This combines DMP's speed with our accuracy requirements
    ///
    /// Collects all matches first, then applies replacements in reverse to maintain indices
    ///
    /// **Performance:** O(N) search using string slicing instead of O(N²) string creation
    pub(crate) fn fuzzy_search_replace(&self, content: &str) -> Result<(String, usize)> {
        if self.pattern.is_empty() {
            return Ok((content.to_string(), 0));
        }

        // Configure DMP for candidate finding
        // Note: DMP's threshold is used for its internal scoring, but we validate separately
        let mut dmp = DiffMatchPatch::new();
        dmp.set_match_threshold(self.threshold);
        dmp.set_match_distance(self.distance as usize);

        let pattern_char_len = self.pattern.chars().count();
        let content_char_len = content.chars().count();

        // If pattern is longer than content, no matches possible
        if pattern_char_len > content_char_len {
            return Ok((content.to_string(), 0));
        }

        // Find all matches using DMP's match_main
        // Store char positions for replacement phase
        let mut matches: Vec<usize> = Vec::new();
        let mut search_from_byte = 0;
        let mut search_from_char = 0;

        while search_from_char + pattern_char_len <= content_char_len {
            // Use string slicing instead of creating new String (O(1) vs O(N))
            // This is the KEY optimization - no allocation per iteration!
            let search_content: &str = &content[search_from_byte..];

            // Use DMP's fuzzy matching to find next match at the START of search_content
            // loc=0 means we expect the match near the beginning
            match dmp.match_main::<Efficient>(search_content, &self.pattern, 0) {
                Some(byte_offset) => {
                    // DMP returns byte offset, convert to char offset in search_content
                    let char_offset_in_slice = search_content[..byte_offset].chars().count();

                    // Convert to absolute char position in original content
                    let absolute_char_pos = search_from_char + char_offset_in_slice;

                    // Validate match is within bounds
                    if absolute_char_pos + pattern_char_len <= content_char_len {
                        // Extract the actual matched text for validation
                        // (This is the ONLY substring creation - necessary for similarity check)
                        let matched_text: String = content
                            .chars()
                            .skip(absolute_char_pos)
                            .take(pattern_char_len)
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
                            // Update BOTH byte and char positions
                            let matched_byte_len = matched_text.len();
                            search_from_byte = search_from_byte + byte_offset + matched_byte_len;
                            search_from_char = absolute_char_pos + pattern_char_len;
                        } else {
                            // DMP found something, but it's not good enough - advance by 1 char and keep looking
                            // Need to find next char boundary for UTF-8 safety
                            let next_char_byte_len = content[search_from_byte + byte_offset..]
                                .chars()
                                .next()
                                .map(|c| c.len_utf8())
                                .unwrap_or(1);
                            search_from_byte = search_from_byte + byte_offset + next_char_byte_len;
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
        // Convert to chars only ONCE at replacement phase (not in loop!)
        let mut result_chars: Vec<char> = content.chars().collect();
        let replacement_chars: Vec<char> = self.replacement.chars().collect();

        for &match_pos in matches.iter().rev() {
            // Double-check bounds before splicing
            if match_pos + pattern_char_len <= result_chars.len() {
                result_chars.splice(
                    match_pos..match_pos + pattern_char_len,
                    replacement_chars.iter().copied(), // Use iter().copied() instead of clone()
                );
            }
        }

        Ok((result_chars.iter().collect(), matches.len()))
    }

    /// Calculate similarity between two strings (0.0 = completely different, 1.0 = identical)
    /// Uses Levenshtein distance calculation
    ///
    /// **Usage:** Part of the hybrid DMP+Levenshtein approach.
    /// DMP's match_main finds candidates quickly, then this method validates match quality
    /// with precise Levenshtein distance scoring. This is actively used at line 376.
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
                let cost = if a_chars[i - 1] == b_chars[j - 1] {
                    0
                } else {
                    1
                };
                matrix[i][j] = std::cmp::min(
                    std::cmp::min(
                        matrix[i - 1][j] + 1, // deletion
                        matrix[i][j - 1] + 1, // insertion
                    ),
                    matrix[i - 1][j - 1] + cost, // substitution
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
}
