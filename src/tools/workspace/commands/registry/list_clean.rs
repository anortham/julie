use super::ManageWorkspaceTool;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use anyhow::Result;
use tracing::info;

impl ManageWorkspaceTool {
    /// Handle list command - show all registered workspaces.
    pub(crate) async fn handle_list_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("Listing all workspaces");

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            // list is orthogonal to having a primary bound — show registered
            // workspaces regardless of session state. CURRENT/PAIRED labels
            // require a primary; without one we fall back to KNOWN for all.
            let primary_workspace_id = handler.current_workspace_id();

            let all_workspaces = match db.list_workspaces() {
                Ok(workspaces) => workspaces,
                Err(e) => {
                    let message = format!("Failed to list workspaces: {}", e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            };
            let paired_ids: std::collections::HashSet<String> = if let Some(ref primary_id) =
                primary_workspace_id
            {
                match db.list_references(primary_id) {
                    Ok(references) => references.into_iter().map(|ws| ws.workspace_id).collect(),
                    Err(e) => {
                        let message = format!("Failed to list workspace pairings: {}", e);
                        return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                    }
                }
            } else {
                std::collections::HashSet::new()
            };

            if all_workspaces.is_empty() {
                let message = "No workspaces registered.";
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
            }

            let mut output = String::from("Registered Workspaces:\n\n");

            for ws in &all_workspaces {
                let path_exists = std::path::Path::new(&ws.path).exists();
                let status_str = if !path_exists { "MISSING" } else { &ws.status };
                let mut labels = Vec::new();
                if Some(ws.workspace_id.as_str()) == primary_workspace_id.as_deref() {
                    labels.push("CURRENT");
                }
                if paired_ids.contains(&ws.workspace_id) {
                    labels.push("PAIRED");
                }
                if labels.is_empty() {
                    labels.push("KNOWN");
                }
                output.push_str(&format!(
                    "{} ({}) [{}]\n\
                     Path: {}\n\
                    Status: {} | Sessions: {}\n\
                     Files: {} | Symbols: {}\n\n",
                    ws.workspace_id
                        .split('_')
                        .next()
                        .unwrap_or(&ws.workspace_id),
                    ws.workspace_id,
                    labels.join(", "),
                    ws.path,
                    status_str,
                    ws.session_count,
                    ws.file_count.unwrap_or(0),
                    ws.symbol_count.unwrap_or(0),
                ));
            }

            return Ok(CallToolResult::text_content(vec![Content::text(output)]));
        }

        // Stdio mode: no workspace registry available
        let message = "No workspaces registered.";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }

    /// Handle clean command - clean expired/orphaned workspaces
    pub(crate) async fn handle_clean_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        info!("Cleaning workspaces (comprehensive cleanup: TTL + Size Limits + Orphans)");

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            let all_workspaces = match db.list_workspaces() {
                Ok(ws) => ws,
                Err(e) => {
                    let message = format!("Failed to list workspaces: {}", e);
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            };

            let mut cleaned_stale = Vec::new();
            let mut cleaned_orphans = Vec::new();

            // Pass 1: Remove DB entries where project path no longer exists
            for ws in &all_workspaces {
                if !std::path::Path::new(&ws.path).exists() {
                    if let Err(e) = db.delete_workspace(&ws.workspace_id) {
                        tracing::warn!(
                            "Failed to delete workspace {} during cleanup: {}",
                            ws.workspace_id,
                            e
                        );
                    } else {
                        cleaned_stale.push(ws.workspace_id.clone());
                    }
                }
            }

            // Pass 2: Remove orphan index directories not tracked in DB
            // Re-fetch workspace list (may have changed from pass 1)
            if let Ok(current_workspaces) = db.list_workspaces() {
                let registered_ids: std::collections::HashSet<String> = current_workspaces
                    .iter()
                    .map(|ws| ws.workspace_id.clone())
                    .collect();

                if let Ok(paths) = crate::paths::DaemonPaths::try_new() {
                    let indexes_dir = paths.indexes_dir();
                    if let Ok(entries) = std::fs::read_dir(&indexes_dir) {
                        for entry in entries.flatten() {
                            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                let dir_name = entry.file_name().to_string_lossy().to_string();
                                if !registered_ids.contains(&dir_name) {
                                    let dir_path = entry.path();
                                    if let Err(e) = std::fs::remove_dir_all(&dir_path) {
                                        tracing::warn!(
                                            "Failed to remove orphan index dir {}: {}",
                                            dir_name,
                                            e
                                        );
                                    } else {
                                        info!("Removed orphan index directory: {}", dir_name);
                                        cleaned_orphans.push(dir_name);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let message = if cleaned_stale.is_empty() && cleaned_orphans.is_empty() {
                "No cleanup needed. All workspaces are healthy!".to_string()
            } else {
                let mut parts = Vec::new();
                if !cleaned_stale.is_empty() {
                    parts.push(format!(
                        "Removed {} stale DB entries (missing paths):\n  {}",
                        cleaned_stale.len(),
                        cleaned_stale.join("\n  "),
                    ));
                }
                if !cleaned_orphans.is_empty() {
                    parts.push(format!(
                        "Removed {} orphan index directories (not in DB):\n  {}",
                        cleaned_orphans.len(),
                        cleaned_orphans.join("\n  "),
                    ));
                }
                parts.join("\n\n")
            };
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        }

        // Stdio mode: no workspace registry available
        let message = "No cleanup needed. All workspaces are healthy!";
        Ok(CallToolResult::text_content(vec![Content::text(message)]))
    }
}
