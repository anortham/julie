//! Rename symbol refactoring operations

use anyhow::Result;
use diff_match_patch_rs::DiffMatchPatch;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use rust_mcp_sdk::schema::CallToolResult;
use tracing::debug;

use super::SmartRefactorTool;
use crate::handler::JulieServerHandler;
use crate::tools::navigation::FastRefsTool;

impl SmartRefactorTool {
    /// Handle rename symbol operation
    pub async fn handle_rename_symbol(&self, handler: &JulieServerHandler) -> Result<CallToolResult> {
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
            .unwrap_or("workspace");

        let update_imports = params
            .get("update_imports")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

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
        let file_locations = self.parse_refs_result(&refs_result)?;

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
        let mut _errors = Vec::new();
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
                    _errors.push(format!("❌ {}: {}", file_path, e));
                }
            }
        }

        // Step 3: Generate result summary
        let total_files = renamed_files.len();
        let total_changes: usize = renamed_files.iter().map(|(_, count)| count).sum();

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

        // Final success message
        let message = format!(
            "Rename successful: '{}' -> '{}'\nModified {} files with {} changes",
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

    /// Parse the result from fast_refs to extract file locations
    pub(crate) fn parse_refs_result(&self, refs_result: &CallToolResult) -> Result<HashMap<String, Vec<u32>>> {
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
