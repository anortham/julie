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
use schemars::JsonSchema;
use std::path::Path;
use crate::mcp_compat::{CallToolResult, Content, CallToolResultExt};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, info};

use crate::tools::editing::EditingTransaction;

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
        workspace_root: &Path,
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
        if let Err(result) = self.validate_parameters() {
            return Ok(result);
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

        // Dry run - show diff preview
        if self.dry_run {
            let diff_preview = Self::generate_diff_preview(&original_content, &modified_content, 10);
            let text = format!(
                "fuzzy_replace dry run — {} match(es) in {}\n'{}' → '{}' (threshold: {})\n\n{}\n\n(dry run — set dry_run=false to apply)",
                matches_found, file_path, self.pattern, self.replacement, self.threshold,
                diff_preview
            );

            debug!("✅ Returning fuzzy_replace dry run preview ({} matches)", matches_found);
            return Ok(CallToolResult::text_content(vec![Content::text(text)]));
        }

        // Apply changes atomically using EditingTransaction
        let transaction = EditingTransaction::begin(&resolved_path_str)
            .map_err(|e| anyhow!("Failed to begin transaction: {}", e))?;
        transaction
            .commit(&modified_content)
            .map_err(|e| anyhow!("Failed to apply changes: {}", e))?;

        let text = format!(
            "fuzzy_replace applied — {} match(es) replaced in {}\n'{}' → '{}' (threshold: {})",
            matches_found, file_path, self.pattern, self.replacement, self.threshold
        );

        Ok(CallToolResult::text_content(vec![Content::text(text)]))
    }

    /// Multi-file mode implementation
    async fn call_tool_multi_file(
        &self,
        file_pattern: &str,
        workspace_root: &Path,
    ) -> Result<CallToolResult> {
        use globset::Glob;

        info!(
            "🔍 Multi-file fuzzy replace: pattern='{}' in files matching '{}' (threshold: {}, distance: {})",
            self.pattern, file_pattern, self.threshold, self.distance
        );

        // Validate parameters
        if let Err(result) = self.validate_parameters() {
            return Ok(result);
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
        let mut per_file_previews = Vec::new();

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

            // Collect per-file diff preview for dry run
            if self.dry_run {
                let rel_path = file_path
                    .strip_prefix(workspace_root)
                    .unwrap_or(file_path)
                    .to_string_lossy();
                let diff = Self::generate_diff_preview(&original_content, &modified_content, 5);
                per_file_previews.push(format!("{} ({} matches)\n{}", rel_path, matches_found, diff));
            }
        }

        let error_text = if !errors.is_empty() {
            format!("\n\nErrors:\n{}", errors.join("\n"))
        } else {
            String::new()
        };

        let text = if self.dry_run {
            let previews = per_file_previews.join("\n\n");
            format!(
                "fuzzy_replace dry run — {} match(es) across {} file(s)\n'{}' → '{}' (threshold: {})\n\n{}{}\n\n(dry run — set dry_run=false to apply)",
                total_matches, files_changed, self.pattern, self.replacement, self.threshold,
                previews, error_text
            )
        } else {
            format!(
                "fuzzy_replace applied — {} match(es) replaced across {} file(s)\n'{}' → '{}' (threshold: {}){}",
                total_matches, files_changed, self.pattern, self.replacement, self.threshold,
                error_text
            )
        };

        Ok(CallToolResult::text_content(vec![Content::text(text)]))
    }

    /// Generate a compact diff preview showing changed lines between original and modified content.
    /// Returns at most `max_diffs` changed line pairs.
    fn generate_diff_preview(original: &str, modified: &str, max_diffs: usize) -> String {
        let orig_lines: Vec<&str> = original.lines().collect();
        let mod_lines: Vec<&str> = modified.lines().collect();

        let mut diffs = Vec::new();
        let mut remaining = 0;

        // When line counts match, do a simple line-by-line comparison
        if orig_lines.len() == mod_lines.len() {
            for (i, (orig, modif)) in orig_lines.iter().zip(mod_lines.iter()).enumerate() {
                if orig != modif {
                    if diffs.len() < max_diffs {
                        diffs.push(format!("  L{}: - {}\n       + {}", i + 1, orig.trim(), modif.trim()));
                    } else {
                        remaining += 1;
                    }
                }
            }
        } else {
            // Line counts differ (pattern/replacement have different newline counts)
            // Show a before/after block instead
            let changed_orig: Vec<(usize, &&str)> = orig_lines.iter().enumerate()
                .filter(|(i, line)| mod_lines.get(*i).map_or(true, |m| m != *line))
                .collect();
            for (i, line) in changed_orig.iter().take(max_diffs) {
                diffs.push(format!("  L{}: - {}", i + 1, line.trim()));
            }
            if changed_orig.len() > max_diffs {
                remaining = changed_orig.len() - max_diffs;
            }
        }

        if remaining > 0 {
            diffs.push(format!("  ... and {} more change(s)", remaining));
        }

        if diffs.is_empty() {
            "  (no visible line changes)".to_string()
        } else {
            diffs.join("\n")
        }
    }
}
