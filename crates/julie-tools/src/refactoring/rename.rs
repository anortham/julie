//! Rename symbol refactoring operations

use anyhow::Result;
use julie_context::ToolContext;
use julie_core::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use serde_json::Value as JsonValue;
use std::collections::HashSet;
use std::path::PathBuf;
use tracing::debug;

use super::staged::{
    PreparedRename, StagedFileRename, build_file_locations, is_import_update_supported,
    normalize_scope_file_path, update_imports_in_content,
};
use super::{RenameChange, RenameSymbolTool, SmartRefactorTool, compute_line_changes};
use crate::navigation::FastRefsTool;
use crate::navigation::resolution::parse_qualified_name;

impl RenameSymbolTool {
    pub fn request_input_bytes(&self) -> u64 {
        serde_json::to_vec(self)
            .map(|bytes| bytes.len() as u64)
            .unwrap_or(0)
    }

    pub fn metrics_metadata_from_prepared(&self, prepared: &PreparedRename) -> JsonValue {
        let reference_count: usize = prepared.files.iter().map(|f| f.changes.len()).sum();
        serde_json::json!({
            "kind": "rename_symbol",
            "dry_run": self.dry_run,
            "applied": !self.dry_run && !prepared.files.is_empty(),
            "input_bytes": self.request_input_bytes(),
            "old_name": self.old_name,
            "new_name": self.new_name,
            "scope": self.scope.as_deref().unwrap_or("workspace"),
            "workspace": self.workspace,
            "reference_count": reference_count,
            "changed_file_count": prepared.files.len(),
            "changed_line_count": prepared.total_changes,
        })
    }

    pub async fn metrics_metadata(&self, handler: &dyn ToolContext) -> Result<JsonValue> {
        let prepared = self.prepare_rename(handler).await?;
        Ok(self.metrics_metadata_from_prepared(&prepared))
    }

    /// Prepares multi-file rename in memory without any disk writes or lock acquisition.
    pub async fn prepare_rename(&self, handler: &dyn ToolContext) -> Result<PreparedRename> {
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
            semantics: None,
        };
        let workspace_target = handler
            .resolve_workspace_target(refs_tool.workspace.as_deref())
            .await?;
        let (definitions, references) = refs_tool
            .find_references_and_definitions(handler, workspace_target)
            .await?;

        let mut file_locations = build_file_locations(&definitions, &references);
        let workspace_root = super::resolve_workspace_root(workspace.as_deref(), handler).await?;
        let scope = self.scope.as_deref().unwrap_or("workspace");

        if scope != "workspace" && scope != "all" {
            if let Some(file_path) = scope.strip_prefix("file:") {
                let normalized_file_path = normalize_scope_file_path(file_path, &workspace_root)?;
                file_locations.retain(|path, _| path == &normalized_file_path);
            } else {
                return Err(anyhow::anyhow!(
                    "Invalid scope '{}'. Must be 'workspace', 'all', or 'file:<path>'",
                    scope
                ));
            }
        }

        let engine = SmartRefactorTool {
            operation: "rename_symbol".to_string(),
            params: "{}".to_string(),
            dry_run: self.dry_run,
        };

        let max_source_bytes = std::env::var("JULIE_MAX_EDIT_SOURCE_BYTES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(16 * 1024 * 1024)
            .clamp(1, 256 * 1024 * 1024);

        let update_imports = false;
        let mut files = Vec::new();
        let mut total_changes = 0;

        for (file_path, lines) in &file_locations {
            let absolute_path = if std::path::Path::new(file_path).is_absolute() {
                PathBuf::from(file_path)
            } else {
                workspace_root.join(file_path)
            };

            let meta = std::fs::metadata(&absolute_path)?;
            if meta.len() > max_source_bytes as u64 {
                return Err(anyhow::anyhow!(
                    "SOURCE_TOO_LARGE: File '{}' size ({} bytes) exceeds limit ({} bytes)",
                    file_path,
                    meta.len(),
                    max_source_bytes
                ));
            }

            let original_content = std::fs::read_to_string(&absolute_path)?;
            let allowed_lines: HashSet<u32> = lines.iter().copied().collect();

            let (mut updated_content, edit_spans) = engine.smart_text_replace_spans(
                &original_content,
                replacement_old_name,
                &self.new_name,
                file_path,
                Some(&allowed_lines),
            )?;

            let mut import_changes_count = 0;
            if update_imports && is_import_update_supported(file_path) {
                let (with_imports, count) = update_imports_in_content(
                    &updated_content,
                    replacement_old_name,
                    &self.new_name,
                )?;
                updated_content = with_imports;
                import_changes_count = count;
            }

            if updated_content != original_content {
                let mut changes = compute_line_changes(&original_content, &updated_content);
                if import_changes_count > 0 {
                    changes.push(RenameChange {
                        line_number: 0,
                        old_line: format!("(+ {} import updates)", import_changes_count),
                        new_line: String::new(),
                    });
                }
                let file_change_count = changes.iter().filter(|c| c.line_number > 0).count();
                total_changes += file_change_count;
                files.push(StagedFileRename {
                    file_path: file_path.clone(),
                    original_content,
                    modified_content: updated_content,
                    changes,
                    edit_spans,
                });
            }
        }

        let import_warning = if update_imports {
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

        debug!(
            "🎯 Prepared rename '{}' -> '{}' across {} file(s) ({} change(s))",
            self.old_name,
            self.new_name,
            files.len(),
            total_changes
        );

        Ok(PreparedRename {
            old_name: self.old_name.clone(),
            new_name: self.new_name.clone(),
            files,
            total_changes,
            import_warning,
        })
    }

    /// Renders dry run preview.
    pub fn format_preview(&self, prepared: &PreparedRename) -> CallToolResult {
        let mut preview_lines: Vec<String> = Vec::new();
        for file in &prepared.files {
            let line_changes: Vec<&RenameChange> =
                file.changes.iter().filter(|c| c.line_number > 0).collect();
            preview_lines.push(format!(
                "  {} ({} changes):",
                file.file_path,
                line_changes.len()
            ));
            for change in file.changes.iter().take(5) {
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
                    preview_lines.push(format!("    {}", change.old_line));
                }
            }
            if file.changes.len() > 5 {
                preview_lines.push(format!(
                    "    ... and {} more changes",
                    file.changes.len() - 5
                ));
            }
        }
        let workspace_label = match &self.workspace {
            Some(ws) if ws != "primary" => format!(" (workspace: {})", ws),
            _ => String::new(),
        };
        let warning_suffix = prepared
            .import_warning
            .as_deref()
            .map(|w| format!("\n\n{}", w))
            .unwrap_or_default();

        let message = format!(
            "rename_symbol dry run{} — '{}' → '{}'\n{} changes across {} files:\n{}\n\n(dry run — no changes applied){}",
            workspace_label,
            prepared.old_name,
            prepared.new_name,
            prepared.total_changes,
            prepared.files.len(),
            preview_lines.join("\n"),
            warning_suffix
        );
        CallToolResult::text_content(vec![Content::text(message)])
    }
}

impl SmartRefactorTool {
    /// Handle rename symbol operation
    pub async fn handle_rename_symbol(&self, handler: &dyn ToolContext) -> Result<CallToolResult> {
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

        let tool = RenameSymbolTool {
            old_name: old_name.to_string(),
            new_name: new_name.to_string(),
            scope: params
                .get("scope")
                .and_then(|v| v.as_str())
                .map(String::from),
            dry_run: self.dry_run,
            workspace: params
                .get("workspace")
                .and_then(|v| v.as_str())
                .map(String::from),
        };

        let prepared = tool.prepare_rename(handler).await?;
        if self.dry_run {
            return Ok(tool.format_preview(&prepared));
        }

        let file_list: Vec<String> = prepared.files.iter().map(|f| f.file_path.clone()).collect();
        let summary = format!(
            "rename_symbol rename_symbol — applied {} change(s) to {}",
            prepared.total_changes,
            file_list.join(", ")
        );
        Ok(CallToolResult::text_content(vec![Content::text(summary)]))
    }
}
