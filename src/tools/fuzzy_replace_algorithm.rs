//! Fuzzy Replace Algorithm - Core matching and replacement logic
//!
//! Extracted from fuzzy_replace.rs for maintainability (500-line limit).
//! Contains the DMP+Levenshtein hybrid approach and structural validation.

use anyhow::Result;
use diff_match_patch_rs::{DiffMatchPatch, Efficient};
use tracing::debug;

use super::fuzzy_replace::FuzzyReplaceTool;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};

impl FuzzyReplaceTool {
    /// Validate threshold and distance parameters.
    /// Returns `Ok(())` if valid, or an error `CallToolResult` if invalid.
    pub(crate) fn validate_parameters(&self) -> Result<(), CallToolResult> {
        if self.threshold < 0.0 || self.threshold > 1.0 {
            return Err(CallToolResult::text_content(vec![Content::text(
                "Error: threshold must be between 0.0 and 1.0 (recommended: 0.8)".to_string(),
            )]));
        }

        if self.distance < 0 {
            return Err(CallToolResult::text_content(vec![Content::text(
                "Error: distance must be positive (recommended: 1000)".to_string(),
            )]));
        }

        Ok(())
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
    /// **Performance:** O(N) search using string slicing instead of O(N^2) string creation
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
                    let char_offset_in_slice =
                        search_content[..valid_byte_offset].chars().count();

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
                        let similarity =
                            self.calculate_similarity(&self.pattern, &matched_text);

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
                    replacement_chars.iter().copied(),
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
    /// with precise Levenshtein distance scoring.
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
