//! Rename symbol refactoring operations

use anyhow::Result;
use diff_match_patch_rs::DiffMatchPatch;
use rust_mcp_sdk::schema::CallToolResult;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use tracing::debug;

use super::SmartRefactorTool;
use crate::handler::JulieServerHandler;
use crate::tools::navigation::FastRefsTool;

impl SmartRefactorTool {
    /// Handle rename symbol operation
    pub async fn handle_rename_symbol(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        debug!("🔄 Processing rename symbol operation");

        // Parse JSON parameters - return errors for invalid JSON or missing parameters
        let params: JsonValue = serde_json::from_str(&self.params)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in params: {}", e))?;

        let old_name = params
            .get("old_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: old_name"))?;

        let new_name = params
            .get("new_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: new_name"))?;

        let scope = params
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("workspace"); // "workspace", "file:<path>", or "all"

        let update_imports = params
            .get("update_imports")
            .and_then(|v| v.as_bool())
            .unwrap_or(false); // Changed default to false for safety

        let update_comments = params
            .get("update_comments")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        debug!(
            "🎯 Rename '{}' -> '{}' (scope: {}, imports: {}, comments: {})",
            old_name, new_name, scope, update_imports, update_comments
        );

        // Step 1: Find all references to the symbol
        let refs_tool = FastRefsTool {
            symbol: old_name.to_string(),
            include_definition: true,
            limit: 1000,                            // High limit for comprehensive rename
            workspace: Some("primary".to_string()), // TODO: Map scope to workspace
        };

        let refs_result = refs_tool.call_tool(handler).await?;

        // Extract file locations from the refs result
        let mut file_locations = self.parse_refs_result(&refs_result)?;

        if file_locations.is_empty() {
            let message = format!("No references found for symbol '{}'", old_name);
            return self.create_result(
                "rename_symbol",
                false, // Failed to find symbol
                vec![],
                0,
                vec![
                    "Use fast_search to locate the symbol".to_string(),
                    "Check spelling of symbol name".to_string(),
                ],
                message,
                None,
            );
        }

        // Apply scope filtering
        if scope != "workspace" && scope != "all" {
            if let Some(file_path) = scope.strip_prefix("file:") {
                // Scope to specific file
                file_locations.retain(|path, _| path == file_path);
                if file_locations.is_empty() {
                    let message = format!(
                        "Symbol '{}' not found in specified file: {}",
                        old_name, file_path
                    );
                    return self.create_result(
                        "rename_symbol",
                        false,
                        vec![],
                        0,
                        vec!["Check the file path is correct".to_string()],
                        message,
                        None,
                    );
                }
                debug!("📍 Scope limited to file: {}", file_path);
            } else {
                return Err(anyhow::anyhow!(
                    "Invalid scope '{}'. Must be 'workspace', 'all', or 'file:<path>'",
                    scope
                ));
            }
        }

        debug!(
            "📍 Found {} references across {} files",
            file_locations
                .values()
                .map(|refs| refs.len())
                .sum::<usize>(),
            file_locations.len()
        );

        // Step 2: Apply renames file by file
        let mut renamed_files = Vec::new();
        let mut errors = Vec::new();
        let dmp = DiffMatchPatch::new();

        for file_path in file_locations.keys() {
            match self
                .rename_in_file(
                    handler,
                    file_path,
                    old_name,
                    new_name,
                    update_comments,
                    &dmp,
                )
                .await
            {
                Ok(changes_applied) => {
                    if changes_applied > 0 {
                        renamed_files.push((file_path.clone(), changes_applied));
                    }
                }
                Err(e) => {
                    errors.push(format!("❌ {}: {}", file_path, e));
                }
            }
        }

        // Step 2.5: Update import statements if requested
        if update_imports && !renamed_files.is_empty() {
            debug!("🔄 Updating import statements for renamed symbol");
            // Pass the files we already found - no need to search again
            let file_paths: Vec<String> = file_locations.keys().cloned().collect();
            match self
                .update_import_statements_in_files(&file_paths, old_name, new_name, &dmp)
                .await
            {
                Ok(updated_files) => {
                    for (file_path, changes) in updated_files {
                        // Add to renamed_files or increment count if already present
                        if let Some((_, existing_changes)) = renamed_files
                            .iter_mut()
                            .find(|(path, _)| path == &file_path)
                        {
                            *existing_changes += changes;
                        } else {
                            renamed_files.push((file_path, changes));
                        }
                    }
                }
                Err(e) => {
                    debug!("⚠️  Failed to update import statements: {}", e);
                    // Don't fail the entire operation, just log the issue
                }
            }
        }

        // Step 3: Generate result summary
        let total_files = renamed_files.len();
        let total_changes: usize = renamed_files.iter().map(|(_, count)| count).sum();

        // FIX: Check for errors and report partial failures
        if !errors.is_empty() {
            let error_summary = errors.join("\n");
            let message = if total_files > 0 {
                format!(
                    "⚠️  Partial rename: '{}' -> '{}'\n\
                    ✅ Successfully modified {} files with {} changes\n\
                    ❌ Failed to modify {} files:\n{}",
                    old_name,
                    new_name,
                    total_files,
                    total_changes,
                    errors.len(),
                    error_summary
                )
            } else {
                format!(
                    "❌ Rename failed: '{}' -> '{}'\n\
                    All {} files failed:\n{}",
                    old_name,
                    new_name,
                    errors.len(),
                    error_summary
                )
            };

            let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
            let warnings = vec![
                "Check file permissions and paths".to_string(),
                "Review errors above for details".to_string(),
                "Use git status to see which files were modified".to_string(),
            ];
            return self.create_result(
                "rename_symbol",
                total_files > 0, // Partial success if some files succeeded
                files,
                total_changes,
                warnings,
                message,
                None,
            );
        }

        if self.dry_run {
            let message = format!(
                "DRY RUN: Rename '{}' -> '{}'\nWould modify {} files with {} changes",
                old_name, new_name, total_files, total_changes
            );

            let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
            return self.create_result(
                "rename_symbol",
                true, // Dry run succeeded
                files,
                total_changes,
                vec!["Set dry_run=false to apply changes".to_string()],
                message,
                None,
            );
        }

        // Final success message (no errors)
        let message = format!(
            "✅ Rename successful: '{}' -> '{}'\nModified {} files with {} changes",
            old_name, new_name, total_files, total_changes
        );

        let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
        self.create_result(
            "rename_symbol",
            true,
            files,
            total_changes,
            vec![
                "Run tests to verify changes".to_string(),
                "Use fast_refs to validate rename completion".to_string(),
                "Use git diff to review changes".to_string(),
            ],
            message,
            None,
        )
    }

    /// Update import statements in the specified files
    /// FIX: Instead of searching the workspace, we directly check the files we already identified
    /// This works for both indexed files and temp test files
    async fn update_import_statements_in_files(
        &self,
        file_paths: &[String],
        old_name: &str,
        new_name: &str,
        dmp: &DiffMatchPatch,
    ) -> Result<Vec<(String, usize)>> {
        let mut updated_files = Vec::new();

        // Check each file directly - no search needed since we already know which files to check
        for file_path in file_paths {
            match self
                .update_imports_in_file(file_path, old_name, new_name, dmp)
                .await
            {
                Ok(changes) if changes > 0 => {
                    debug!("✅ Updated {} import(s) in {}", changes, file_path);
                    updated_files.push((file_path.clone(), changes));
                }
                Ok(_) => {
                    // No import changes needed in this file
                }
                Err(e) => {
                    debug!("⚠️  Failed to update imports in {}: {}", file_path, e);
                }
            }
        }

        Ok(updated_files)
    }

    /// Update imports in a single file
    async fn update_imports_in_file(
        &self,
        file_path: &str,
        old_name: &str,
        new_name: &str,
        _dmp: &DiffMatchPatch,
    ) -> Result<usize> {
        use regex::Regex;

        let content = std::fs::read_to_string(file_path)?;
        let mut changes = 0;

        // Build regex patterns with word boundaries to avoid partial matches
        // \b ensures we match whole identifiers, not substrings like getUserData in getUserDataFromCache
        let patterns = vec![
            // JavaScript/TypeScript: import { getUserData } from 'module'
            Regex::new(&format!(
                r"\bimport\s+\{{\s*{}\s*\}}",
                regex::escape(old_name)
            ))?,
            // JavaScript/TypeScript: import { getUserData, other } (leading position)
            Regex::new(&format!(
                r"\bimport\s+\{{\s*{}\s*,",
                regex::escape(old_name)
            ))?,
            // JavaScript/TypeScript: import { other, getUserData } (trailing position)
            Regex::new(&format!(r",\s*{}\s*\}}", regex::escape(old_name)))?,
            // Python: from module import getUserData (word boundary)
            Regex::new(&format!(
                r"\bfrom\s+\S+\s+import\s+{}\b",
                regex::escape(old_name)
            ))?,
            // Rust: use module::getUserData (word boundary)
            Regex::new(&format!(r"\buse\s+.*::{}\b", regex::escape(old_name)))?,
        ];

        let mut modified_content = content.clone();

        for regex in patterns {
            if regex.is_match(&modified_content) {
                let before = modified_content.clone();

                // Use regex replace_all with callback to replace old_name with new_name
                // This preserves the rest of the matched pattern (imports, from, use keywords, etc.)
                modified_content = regex
                    .replace_all(&modified_content, |caps: &regex::Captures| {
                        caps[0].replace(old_name, new_name)
                    })
                    .to_string();

                if modified_content != before {
                    changes += 1;
                }
            }
        }

        if changes > 0 && !self.dry_run {
            use crate::tools::editing::EditingTransaction;
            let tx = EditingTransaction::begin(file_path)?;
            tx.commit(&modified_content)?;
        }

        Ok(changes)
    }

    /// Parse the result from fast_refs to extract file locations
    pub(crate) fn parse_refs_result(
        &self,
        refs_result: &CallToolResult,
    ) -> Result<HashMap<String, Vec<u32>>> {
        let mut file_locations: HashMap<String, Vec<u32>> = HashMap::new();

        // Extract text content from the result
        let content = refs_result
            .content
            .iter()
            .filter_map(|block| {
                if let Ok(json_value) = serde_json::to_value(block) {
                    json_value
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Prefer structured payloads when available
        if let Some(structured) = &refs_result.structured_content {
            if let Some(references) = structured.get("references").and_then(|v| v.as_array()) {
                for reference in references {
                    if let (Some(file_path), Some(line_number)) = (
                        reference.get("file_path").and_then(|v| v.as_str()),
                        reference.get("line_number").and_then(|v| v.as_u64()),
                    ) {
                        file_locations
                            .entry(file_path.to_string())
                            .or_default()
                            .push(line_number as u32);
                    }
                }
            }

            if let Some(definitions) = structured.get("definitions").and_then(|v| v.as_array()) {
                for definition in definitions {
                    if let (Some(file_path), Some(line_number)) = (
                        definition.get("file_path").and_then(|v| v.as_str()),
                        definition.get("start_line").and_then(|v| v.as_u64()),
                    ) {
                        file_locations
                            .entry(file_path.to_string())
                            .or_default()
                            .push(line_number as u32);
                    }
                }
            }

            if !file_locations.is_empty() {
                return Ok(file_locations);
            }
        }

        // Parse textual fallback (expected format: "file_path:line_number")
        for line in content.lines() {
            let after_dash = line
                .split_once(" - ")
                .map(|(_, rest)| rest)
                .unwrap_or_else(|| line.trim());

            let mut selected: Option<(&str, &str)> = None;
            for (idx, _) in after_dash.match_indices(':') {
                if let Some(remainder) = after_dash.get(idx + 1..) {
                    let trimmed = remainder.trim_start();
                    let digit_count = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
                    // Check if we have at least one digit after the colon
                    if digit_count > 0 {
                        // Looks like a line number (digits followed by optional non-digits like " (confidence: 0.95)")
                        selected = Some(after_dash.split_at(idx));
                        break; // Use the FIRST colon with digits, not the last
                    }
                }
            }

            if let Some((file_part, line_part)) = selected {
                // Extract only the leading digits from line_part (handles suffixes like " (confidence: 0.95)")
                let digits_only: String = line_part
                    .trim_start_matches(':')
                    .trim_start()
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();

                if let Ok(line_num) = digits_only.parse::<u32>() {
                    file_locations
                        .entry(file_part.to_string())
                        .or_default()
                        .push(line_num);
                }
            }
        }

        Ok(file_locations)
    }
}
