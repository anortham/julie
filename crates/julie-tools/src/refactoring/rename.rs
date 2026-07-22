//! Rename symbol refactoring operations

use anyhow::Result;
use julie_core::mcp_compat::CallToolResult;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use tracing::debug;

use super::{RenameChange, RenameSymbolTool, SmartRefactorTool, compute_line_changes};
use crate::navigation::FastRefsTool;
use crate::navigation::resolution::parse_qualified_name;
use julie_context::ToolContext;
use julie_extractors::{Relationship, Symbol};

impl RenameSymbolTool {
    pub fn request_input_bytes(&self) -> u64 {
        serde_json::to_vec(self)
            .map(|bytes| bytes.len() as u64)
            .unwrap_or(0)
    }

    pub async fn metrics_metadata(&self, handler: &dyn ToolContext) -> Result<JsonValue> {
        let workspace = self
            .workspace
            .clone()
            .or_else(|| Some("primary".to_string()));
        let replacement_old_name = parse_qualified_name(&self.old_name)
            .map(|(_, child)| child)
            .unwrap_or(&self.old_name);

        let refs_tool = FastRefsTool {
            symbol: self.old_name.clone(),
            include_definition: true,
            limit: 1000,
            workspace: workspace.clone(),
            reference_kind: None,
        };
        let workspace_target = handler
            .resolve_workspace_target(refs_tool.workspace.as_deref())
            .await?;
        let (definitions, references) = refs_tool
            .find_references_and_definitions(handler, workspace_target)
            .await?;

        let mut file_locations = build_file_locations(&definitions, &references);
        let reference_count: usize = file_locations.values().map(Vec::len).sum();
        let workspace_root = super::resolve_workspace_root(workspace.as_deref(), handler).await?;
        let scope = self.scope.as_deref().unwrap_or("workspace");

        if scope != "workspace" && scope != "all" {
            if let Some(file_path) = scope.strip_prefix("file:") {
                let normalized_file_path = normalize_scope_file_path(file_path, &workspace_root)?;
                file_locations.retain(|path, _| path == &normalized_file_path);
            }
        }

        let engine = SmartRefactorTool {
            operation: "rename_symbol".to_string(),
            params: "{}".to_string(),
            dry_run: true,
        };
        let mut changed_file_count = 0usize;
        let mut changed_line_count = 0usize;

        for (file_path, lines) in &file_locations {
            let absolute_path = if std::path::Path::new(file_path).is_absolute() {
                std::path::PathBuf::from(file_path)
            } else {
                workspace_root.join(file_path)
            };
            let content = std::fs::read_to_string(&absolute_path)?;
            let allowed_lines: HashSet<u32> = lines.iter().copied().collect();
            let updated_content = engine.smart_text_replace_on_lines(
                &content,
                replacement_old_name,
                &self.new_name,
                file_path,
                false,
                &allowed_lines,
            )?;
            if updated_content != content {
                changed_file_count += 1;
                changed_line_count += compute_line_changes(&content, &updated_content)
                    .into_iter()
                    .filter(|change| change.line_number > 0)
                    .count();
            }
        }

        Ok(serde_json::json!({
            "kind": "rename_symbol",
            "dry_run": self.dry_run,
            "applied": !self.dry_run && changed_file_count > 0,
            "input_bytes": self.request_input_bytes(),
            "old_name": self.old_name,
            "new_name": self.new_name,
            "scope": scope,
            "reference_count": reference_count,
            "changed_file_count": changed_file_count,
            "changed_line_count": changed_line_count,
        }))
    }
}

impl SmartRefactorTool {
    /// Handle rename symbol operation
    pub async fn handle_rename_symbol(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
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

        let workspace = params
            .get("workspace")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        debug!(
            "🎯 Rename '{}' -> '{}' (scope: {}, imports: {}, comments: {}, workspace: {:?})",
            old_name, new_name, scope, update_imports, update_comments, workspace
        );
        let replacement_old_name = parse_qualified_name(old_name)
            .map(|(_, child)| child)
            .unwrap_or(old_name);

        // Step 1: Find all references to the symbol
        let refs_tool = FastRefsTool {
            symbol: old_name.to_string(),
            include_definition: true,
            limit: 1000, // High limit for comprehensive rename
            workspace: workspace.clone().or_else(|| Some("primary".to_string())),
            reference_kind: None, // No filtering - find all reference kinds
        };

        let workspace_target = handler
            .resolve_workspace_target(refs_tool.workspace.as_deref())
            .await?;
        let (definitions, references) = refs_tool
            .find_references_and_definitions(handler, workspace_target)
            .await?;

        // Build file -> line-number map directly from structured data (no text parsing)
        let mut file_locations = build_file_locations(&definitions, &references);

        if file_locations.is_empty() {
            return self.create_result(
                "rename_symbol",
                false,
                vec![],
                0,
                Some(format!(
                    "rename_symbol: no references found for '{}'\nCheck symbol name spelling or use fast_search to locate it.",
                    old_name
                )),
            );
        }

        let workspace_root = super::resolve_workspace_root(workspace.as_deref(), handler).await?;

        // Apply scope filtering
        if scope != "workspace" && scope != "all" {
            if let Some(file_path) = scope.strip_prefix("file:") {
                let normalized_file_path = normalize_scope_file_path(file_path, &workspace_root)?;
                // Scope to specific file
                file_locations.retain(|path, _| path == &normalized_file_path);
                if file_locations.is_empty() {
                    return self.create_result(
                        "rename_symbol",
                        false,
                        vec![],
                        0,
                        Some(format!(
                            "rename_symbol: '{}' not found in scope '{}'",
                            old_name, scope
                        )),
                    );
                }
                debug!("📍 Scope limited to file: {}", normalized_file_path);
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
        let mut renamed_files: Vec<(String, Vec<RenameChange>)> = Vec::new();
        let mut errors = Vec::new();

        for (file_path, lines) in &file_locations {
            match self
                .rename_in_file(
                    &workspace_root,
                    file_path,
                    replacement_old_name,
                    new_name,
                    lines,
                )
                .await
            {
                Ok(changes) => {
                    if !changes.is_empty() {
                        renamed_files.push((file_path.clone(), changes));
                    }
                }
                Err(e) => {
                    errors.push(format!("❌ {}: {}", file_path, e));
                }
            }
        }

        // Step 2.5: Update import statements if requested
        // Warn upfront when files contain languages not supported by import rewriting.
        // Supported: JS/TS (.js/.ts/.jsx/.tsx), Python (.py), Rust (.rs).
        let import_unsupported_warning = if update_imports {
            let unsupported_exts: std::collections::BTreeSet<String> = file_locations
                .keys()
                .filter(|p| !is_import_update_supported(p))
                .filter_map(|p| p.rsplit('.').next().map(|e| format!(".{}", e)))
                .collect();
            if unsupported_exts.is_empty() {
                None
            } else {
                Some(format!(
                    "Note: update_imports only supports JS/TS, Python, and Rust. \
                     Files with unsupported extensions ({}) were skipped for import rewriting.",
                    unsupported_exts.into_iter().collect::<Vec<_>>().join(", ")
                ))
            }
        } else {
            None
        };

        if update_imports && !renamed_files.is_empty() {
            debug!("Updating import statements for renamed symbol");
            let file_paths: Vec<String> = file_locations.keys().cloned().collect();
            match self
                .update_import_statements_in_files(&workspace_root, &file_paths, old_name, new_name)
                .await
            {
                Ok(updated_files) => {
                    for (file_path, changes_count) in updated_files {
                        // Import changes don't have line-level detail — just add a note
                        let import_note = RenameChange {
                            line_number: 0,
                            old_line: format!("(+ {} import updates)", changes_count),
                            new_line: String::new(),
                        };
                        if let Some((_, existing_changes)) = renamed_files
                            .iter_mut()
                            .find(|(path, _)| path == &file_path)
                        {
                            existing_changes.push(import_note);
                        } else {
                            renamed_files.push((file_path, vec![import_note]));
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
        let total_changes: usize = renamed_files
            .iter()
            .map(|(_, changes)| changes.iter().filter(|c| c.line_number > 0).count())
            .sum();

        // Check for errors and report partial failures
        if !errors.is_empty() {
            let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
            let error_text = errors.join("\n");
            let warning_suffix = import_unsupported_warning
                .as_deref()
                .map(|w| format!("\n{}", w))
                .unwrap_or_default();
            return self.create_result(
                "rename_symbol",
                total_files > 0,
                files.clone(),
                total_changes,
                Some(format!(
                    "rename_symbol: partial failure renaming '{}' → '{}'\n{} changes in {} files, but errors occurred:\n{}{}",
                    old_name, new_name, total_changes, files.len(), error_text, warning_suffix
                )),
            );
        }

        if self.dry_run {
            let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
            let mut preview_lines: Vec<String> = Vec::new();
            for (file_path, changes) in &renamed_files {
                let line_changes: Vec<&RenameChange> =
                    changes.iter().filter(|c| c.line_number > 0).collect();
                preview_lines.push(format!("  {} ({} changes):", file_path, line_changes.len()));
                for change in changes.iter().take(5) {
                    if change.line_number > 0 {
                        preview_lines.push(format!(
                            "    L{}: - {}",
                            change.line_number,
                            change.old_line.trim()
                        ));
                        preview_lines.push(format!(
                            "    L{}: + {}",
                            change.line_number,
                            change.new_line.trim()
                        ));
                    } else {
                        preview_lines.push(format!("    {}", change.old_line)); // import update note
                    }
                }
                if changes.len() > 5 {
                    preview_lines.push(format!("    ... and {} more changes", changes.len() - 5));
                }
            }
            let workspace_label = match &workspace {
                Some(ws) if ws != "primary" => format!(" (workspace: {})", ws),
                _ => String::new(),
            };
            let warning_suffix = import_unsupported_warning
                .as_deref()
                .map(|w| format!("\n\n{}", w))
                .unwrap_or_default();
            return self.create_result(
                "rename_symbol",
                true,
                files,
                total_changes,
                Some(format!(
                    "rename_symbol dry run{} — '{}' → '{}'\n{} changes across {} files:\n{}\n\n(dry run — no changes applied){}",
                    workspace_label, old_name, new_name, total_changes, renamed_files.len(),
                    preview_lines.join("\n"), warning_suffix
                )),
            );
        }

        let files: Vec<String> = renamed_files.iter().map(|(f, _)| f.clone()).collect();
        self.create_result(
            "rename_symbol",
            true,
            files,
            total_changes,
            import_unsupported_warning, // Surface warning even on success when languages unsupported
        )
    }

    /// Update import statements in the specified files
    /// FIX: Instead of searching the workspace, we directly check the files we already identified
    /// This works for both indexed files and temp test files
    async fn update_import_statements_in_files(
        &self,
        workspace_root: &std::path::Path,
        file_paths: &[String],
        old_name: &str,
        new_name: &str,
    ) -> Result<Vec<(String, usize)>> {
        let mut updated_files = Vec::new();

        for file_path in file_paths {
            match self
                .update_imports_in_file(workspace_root, file_path, old_name, new_name)
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
        workspace_root: &std::path::Path,
        file_path: &str,
        old_name: &str,
        new_name: &str,
    ) -> Result<usize> {
        use regex::Regex;

        // Resolve file path relative to workspace root
        let absolute_path = if std::path::Path::new(file_path).is_absolute() {
            file_path.to_string()
        } else {
            workspace_root.join(file_path).to_string_lossy().to_string()
        };

        let content = std::fs::read_to_string(&absolute_path)?;
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
            use crate::editing::EditingTransaction;
            let tx = EditingTransaction::begin(&absolute_path)?;
            tx.commit_if_unchanged(&modified_content, &content)?;
        }

        Ok(changes)
    }
}

/// Returns true if the file extension is supported for automatic import rewriting.
/// Supported: JavaScript/TypeScript (.js/.ts/.jsx/.tsx/.mjs/.cjs), Python (.py), Rust (.rs).
fn is_import_update_supported(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("");
    matches!(
        ext,
        "js" | "ts" | "tsx" | "jsx" | "mjs" | "cjs" | "py" | "rs"
    )
}

/// Normalize a `file:` scope argument to a workspace-relative Unix-style path.
///
/// Strict contract: rejects paths outside the workspace root with a typed
/// `WorkspaceResolutionFailure`. The MCP boundary surfaces this as
/// `invalid_params`. No raw-input fallback — silently rewriting `\` → `/` on an
/// outside-workspace path would let it match a coincidentally-named file inside
/// the workspace and rename code the user never intended to touch.
fn normalize_scope_file_path(file_path: &str, workspace_root: &std::path::Path) -> Result<String> {
    let resolution = julie_core::paths::resolve_workspace_file_input(file_path, workspace_root)?;
    Ok(resolution.relative_query_path)
}

/// Used by rename to find all locations that need to be updated.
fn build_file_locations(
    definitions: &[Symbol],
    references: &[Relationship],
) -> HashMap<String, Vec<u32>> {
    let mut file_locations: HashMap<String, Vec<u32>> = HashMap::new();
    for def in definitions {
        file_locations
            .entry(def.file_path.clone())
            .or_default()
            .push(def.start_line);
    }
    for rel in references {
        file_locations
            .entry(rel.file_path.clone())
            .or_default()
            .push(rel.line_number);
    }
    file_locations
}
