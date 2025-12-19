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

use anyhow::{Result, anyhow};
use diff_match_patch_rs::{DiffMatchPatch, Efficient};
use schemars::JsonSchema;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt, WithStructuredContent};
use serde::{Deserialize, Serialize};
use tokio::fs; // Use async file I/O
use tracing::{debug, info};

use crate::tools::editing::EditingTransaction;

/// Structured result from fuzzy replace operation
#[derive(Debug, Clone, Serialize)]
pub struct FuzzyReplaceResult {
    pub tool: String,
    pub file_path: Option<String>,    // Single-file mode
    pub file_pattern: Option<String>, // Multi-file mode
    pub files_changed: usize,         // For multi-file mode
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

fn default_dry_run() -> bool {
    true
}

//**********************//
//   Fuzzy Replace Tool //
//**********************//

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct FuzzyReplaceTool {
    /// File path for single-file mode (omit when using file_pattern)
    #[serde(default)]
    pub file_path: Option<String>,
    /// Glob pattern for multi-file mode (omit when using file_path)
    #[serde(default)]
    pub file_pattern: Option<String>,
    /// Pattern to find
    pub pattern: String,
    /// Replacement text
    pub replacement: String,
    /// Fuzzy match threshold (default: 0.8, range: 0.0-1.0)
    #[serde(default = "default_threshold")]
    pub threshold: f32,
    /// Match distance in characters (default: 1000)
    #[serde(default = "default_distance")]
    pub distance: i32,
    /// Preview without applying (default: true)
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
    /// Validate structural integrity (default: true)
    #[serde(default = "default_true")]
    pub validate: bool,
}

impl FuzzyReplaceTool {
    pub async fn call_tool(
        &self,
        handler: &crate::handler::JulieServerHandler,
    ) -> Result<CallToolResult> {
        use std::env;

        // VALIDATION: Require exactly ONE of file_path or file_pattern
        match (&self.file_path, &self.file_pattern) {
            (None, None) => {
                return Ok(CallToolResult::text_content(vec![Content::text(
                    "Error: Must provide exactly one of file_path (single-file mode) or file_pattern (multi-file mode)".to_string(),
                )]));
            }
            (Some(_), Some(_)) => {
                return Ok(CallToolResult::text_content(vec![Content::text(
                    "Error: Cannot provide both file_path and file_pattern. Use file_path for single-file mode OR file_pattern for multi-file mode.".to_string(),
                )]));
            }
            _ => {} // Exactly one provided - valid
        }

        // Get workspace root
        let workspace_root = if let Some(workspace) = handler.get_workspace().await? {
            workspace.root.clone()
        } else {
            env::current_dir()
                .map_err(|e| anyhow!("Failed to determine current directory: {}", e))?
        };

        // Route to single-file or multi-file implementation
        if let Some(ref file_path) = self.file_path {
            // SINGLE-FILE MODE
            self.call_tool_single_file(file_path, &workspace_root).await
        } else if let Some(ref file_pattern) = self.file_pattern {
            // MULTI-FILE MODE
            self.call_tool_multi_file(file_pattern, &workspace_root)
                .await
        } else {
            unreachable!("Validation ensures exactly one parameter is provided")
        }
    }

    /// Single-file mode implementation (original logic)
    async fn call_tool_single_file(
        &self,
        file_path: &str,
        workspace_root: &std::path::PathBuf,
    ) -> Result<CallToolResult> {
        use crate::utils::file_utils::secure_path_resolution;

        let resolved_path = secure_path_resolution(file_path, workspace_root)?;
        let resolved_path_str = resolved_path.to_string_lossy().to_string();

        info!(
            "🔍 Fuzzy replace in: {} (threshold: {}, distance: {})",
            resolved_path.display(),
            self.threshold,
            self.distance
        );

        // Validate parameters
        if self.threshold < 0.0 || self.threshold > 1.0 {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: threshold must be between 0.0 and 1.0 (recommended: 0.8)".to_string(),
            )]));
        }

        if self.distance < 0 {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: distance must be positive (recommended: 1000)".to_string(),
            )]));
        }

        // Read file (async to avoid blocking executor thread)
        let original_content = fs::read_to_string(&resolved_path)
            .await
            .map_err(|e| anyhow!("Failed to read file '{}': {}", resolved_path.display(), e))?;

        if original_content.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!("Error: File is empty: {}", resolved_path.display()),
            )]));
        }

        // Perform fuzzy search and replace
        let (modified_content, matches_found) = self.fuzzy_search_replace(&original_content)?;

        if matches_found == 0 {
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!(
                    "No fuzzy matches found for pattern in: {}\nPattern: '{}', Threshold: {} (try increasing threshold or distance)",
                    file_path, self.pattern, self.threshold
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
                    return Ok(CallToolResult::text_content(vec![Content::text(
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
                file_path: Some(resolved_path.display().to_string()),
                file_pattern: None,
                files_changed: 1,
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

            // Minimal 2-line summary (unused since we return JSON-only now)
            let _markdown = format!(
                "Fuzzy match preview: {} matches found in {}\nPattern: '{}' → Replacement: '{}' (threshold: {}, dry_run: true)",
                result.matches_found,
                result.file_path.as_ref().unwrap(),
                result.pattern,
                result.replacement,
                result.threshold
            );

            // Return ONLY structured JSON (no redundant text)
            let structured = serde_json::to_value(&result)
                .map_err(|e| anyhow!("Failed to serialize result: {}", e))?;

            let structured_map = if let serde_json::Value::Object(map) = structured {
                map
            } else {
                return Err(anyhow!("Expected JSON object"));
            };

            debug!("✅ Returning fuzzy_replace results as JSON-only (no redundant text_content)");
            return Ok(
                CallToolResult::text_content(vec![])
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
            file_path: Some(resolved_path.display().to_string()),
            file_pattern: None,
            files_changed: 1,
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
            result.matches_found,
            result.file_path.as_ref().unwrap(),
            result.pattern,
            result.replacement,
            result.threshold
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
            CallToolResult::text_content(vec![Content::text(markdown)])
                .with_structured_content(structured_map),
        )
    }

    /// Multi-file mode implementation (NEW)
    async fn call_tool_multi_file(
        &self,
        file_pattern: &str,
        workspace_root: &std::path::PathBuf,
    ) -> Result<CallToolResult> {
        use globset::Glob;

        info!(
            "🔍 Multi-file fuzzy replace: pattern='{}' in files matching '{}' (threshold: {}, distance: {})",
            self.pattern, file_pattern, self.threshold, self.distance
        );

        // Validate parameters
        if self.threshold < 0.0 || self.threshold > 1.0 {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: threshold must be between 0.0 and 1.0 (recommended: 0.8)".to_string(),
            )]));
        }

        if self.distance < 0 {
            return Ok(CallToolResult::text_content(vec![Content::text(
                "Error: distance must be positive (recommended: 1000)".to_string(),
            )]));
        }

        // Build glob matcher
        let glob = Glob::new(file_pattern)
            .map_err(|e| anyhow!("Invalid glob pattern '{}': {}", file_pattern, e))?;
        let matcher = glob.compile_matcher();

        // Find all matching files using walkdir
        let mut matching_files = Vec::new();
        for entry in walkdir::WalkDir::new(workspace_root)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                // Get path relative to workspace root for glob matching
                let relative_path = entry
                    .path()
                    .strip_prefix(workspace_root)
                    .unwrap_or(entry.path());

                // Normalize to Unix-style paths for glob matching (Windows compatibility)
                let path_str = relative_path.to_string_lossy();
                let normalized_path = path_str.replace('\\', "/");

                if matcher.is_match(&normalized_path) {
                    matching_files.push(entry.path().to_path_buf());
                }
            }
        }

        if matching_files.is_empty() {
            return Ok(CallToolResult::text_content(vec![Content::text(
                format!("No files found matching pattern: '{}'", file_pattern),
            )]));
        }

        info!(
            "Found {} files matching pattern '{}'",
            matching_files.len(),
            file_pattern
        );

        // Process each file
        let mut total_matches = 0;
        let mut files_changed = 0;
        let mut errors = Vec::new();

        for file_path in &matching_files {
            info!("🔄 Processing file: {}", file_path.display());

            // Read file
            let original_content = match fs::read_to_string(file_path).await {
                Ok(content) => content,
                Err(e) => {
                    errors.push(format!("Failed to read '{}': {}", file_path.display(), e));
                    continue;
                }
            };

            if original_content.is_empty() {
                continue; // Skip empty files
            }

            // Perform fuzzy search and replace
            info!(
                "  🔍 Running fuzzy search on {} ({} bytes)",
                file_path.display(),
                original_content.len()
            );
            let (modified_content, matches_found) =
                match self.fuzzy_search_replace(&original_content) {
                    Ok(result) => result,
                    Err(e) => {
                        errors.push(format!(
                            "Failed to process '{}': {}",
                            file_path.display(),
                            e
                        ));
                        continue;
                    }
                };
            info!(
                "  ✅ Found {} matches in {}",
                matches_found,
                file_path.display()
            );

            if matches_found == 0 {
                continue; // Skip files with no matches
            }

            // Validate if requested
            if self.validate {
                let original_balance = self.calculate_balance(&original_content);
                let modified_balance = self.calculate_balance(&modified_content);

                if original_balance != modified_balance {
                    let replacement_balance = self.calculate_balance(&self.replacement);
                    let pattern_balance = self.calculate_balance(&self.pattern);

                    if replacement_balance != pattern_balance {
                        errors.push(format!(
                            "Validation failed in '{}': Replacement changes bracket/paren balance",
                            file_path.display()
                        ));
                        continue;
                    }
                }
            }

            // Apply changes (if not dry run)
            if !self.dry_run {
                let transaction = EditingTransaction::begin(file_path.to_string_lossy().as_ref())
                    .map_err(|e| {
                    anyhow!(
                        "Failed to begin transaction for '{}': {}",
                        file_path.display(),
                        e
                    )
                })?;
                transaction.commit(&modified_content).map_err(|e| {
                    anyhow!(
                        "Failed to commit changes to '{}': {}",
                        file_path.display(),
                        e
                    )
                })?;
            }

            total_matches += matches_found;
            files_changed += 1;
        }

        // Create structured result
        let result = FuzzyReplaceResult {
            tool: "fuzzy_replace".to_string(),
            file_path: None,
            file_pattern: Some(file_pattern.to_string()),
            files_changed,
            pattern: self.pattern.clone(),
            replacement: self.replacement.clone(),
            matches_found: total_matches,
            threshold: self.threshold,
            dry_run: self.dry_run,
            validation_passed: errors.is_empty(),
            next_actions: if self.dry_run {
                vec![
                    format!(
                        "Review: {} matches across {} files",
                        total_matches, files_changed
                    ),
                    "Set dry_run=false to apply changes".to_string(),
                ]
            } else {
                vec![
                    format!(
                        "Replaced {} matches across {} files",
                        total_matches, files_changed
                    ),
                    "Run tests to verify changes".to_string(),
                ]
            },
        };

        // Build summary
        let mode = if self.dry_run { "preview" } else { "complete" };
        let mut markdown = format!(
            "Multi-file fuzzy replace {}: {} matches across {} files\nPattern: '{}' → '{}' (pattern: '{}', threshold: {}, dry_run: {})",
            mode,
            total_matches,
            files_changed,
            self.pattern,
            self.replacement,
            file_pattern,
            self.threshold,
            self.dry_run
        );

        if !errors.is_empty() {
            markdown.push_str(&format!("\n\nErrors ({}):", errors.len()));
            for error in errors.iter().take(5) {
                markdown.push_str(&format!("\n- {}", error));
            }
            if errors.len() > 5 {
                markdown.push_str(&format!("\n... and {} more", errors.len() - 5));
            }
        }

        // Return ONLY structured JSON (no redundant text)
        let structured = serde_json::to_value(&result)
            .map_err(|e| anyhow!("Failed to serialize result: {}", e))?;

        let structured_map = if let serde_json::Value::Object(map) = structured {
            map
        } else {
            return Err(anyhow!("Expected JSON object"));
        };

        debug!("✅ Returning fuzzy_replace (multi-file) results as JSON-only (no redundant text_content)");
        Ok(
            CallToolResult::text_content(vec![])
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
                    // DMP returns byte offset, but it may not be on a UTF-8 character boundary
                    // Find the valid character boundary at or before byte_offset
                    let valid_byte_offset = if byte_offset == 0 {
                        0
                    } else if search_content.is_char_boundary(byte_offset) {
                        byte_offset
                    } else {
                        // Find the previous character boundary
                        (0..byte_offset)
                            .rev()
                            .find(|&i| search_content.is_char_boundary(i))
                            .unwrap_or(0)
                    };

                    // Convert to char offset using the valid boundary
                    let char_offset_in_slice = search_content[..valid_byte_offset].chars().count();

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
                            search_from_byte =
                                search_from_byte + valid_byte_offset + matched_byte_len;
                            search_from_char = absolute_char_pos + pattern_char_len;
                        } else {
                            // DMP found something, but it's not good enough - advance by 1 char and keep looking
                            // Need to find next char boundary for UTF-8 safety
                            let next_char_byte_len = content
                                [search_from_byte + valid_byte_offset..]
                                .chars()
                                .next()
                                .map(|c| c.len_utf8())
                                .unwrap_or(1);
                            search_from_byte =
                                search_from_byte + valid_byte_offset + next_char_byte_len;
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
