use super::ManageWorkspaceTool;
use super::cleanup::{
    CLEANUP_ACTION_MANUAL_DELETE, CLEANUP_REASON_USER_REQUEST, WorkspaceDeleteOutcome,
    delete_workspace_if_allowed, prune_missing_workspaces,
};
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace::registry::generate_workspace_id;
use anyhow::{Result, anyhow};
use tracing::{info, warn};

impl ManageWorkspaceTool {
    /// Handle register command - register a known workspace and index it
    /// without activating it for the current session.
    pub(crate) async fn handle_register_command(
        &self,
        handler: &JulieServerHandler,
        path: &str,
        name: Option<String>,
    ) -> Result<CallToolResult> {
        let Some(db) = handler.daemon_db.as_ref() else {
            let message = "Workspace registration requires daemon mode. Start the daemon with `julie daemon`.";
            return Ok(CallToolResult::error(vec![Content::text(message)]));
        };

        if let Err(error) = prune_missing_workspaces(
            db,
            handler.workspace_pool.as_ref(),
            handler.watcher_pool.as_ref(),
        )
        .await
        {
            warn!(
                path = %path,
                error = %error,
                "Workspace cleanup sweep failed during register"
            );
        }

        let expanded = shellexpand::tilde(path).to_string();
        let workspace_path = std::path::PathBuf::from(expanded);
        let canonical_path = workspace_path
            .canonicalize()
            .map_err(|e| anyhow!("Failed to canonicalize workspace path '{}': {e}", path))?;
        let canonical_path_str = canonical_path.to_string_lossy().to_string();

        let existing = db.get_workspace_by_path(&canonical_path_str)?;
        let workspace_id = existing
            .as_ref()
            .map(|row| row.workspace_id.clone())
            .unwrap_or(generate_workspace_id(&canonical_path_str)?);

        let dir_name = canonical_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&workspace_id);
        let display_name = name.unwrap_or_else(|| dir_name.to_string());
        let force = self.force.unwrap_or(false);

        if let Some(existing) = existing.as_ref() {
            if existing.status == "ready" && !force {
                let message = format!(
                    "Workspace Registered\n\
                     Workspace ID: {}\n\
                     Display Name: {}\n\
                     Path: {}\n\
                     Files: {} | Symbols: {}\n\
                     Use manage_workspace(operation=\"open\", workspace_id=\"{}\") to make it current in this session.",
                    workspace_id,
                    display_name,
                    existing.path,
                    existing.file_count.unwrap_or(0),
                    existing.symbol_count.unwrap_or(0),
                    workspace_id,
                );
                return Ok(CallToolResult::text_content(vec![Content::text(message)]));
            }
        }

        db.upsert_workspace(&workspace_id, &canonical_path_str, "indexing")?;
        info!(
            workspace_id = %workspace_id,
            path = %canonical_path_str,
            "Registering workspace and building index"
        );

        match self
            .index_workspace_files(handler, &canonical_path, force)
            .await
        {
            Ok(result) => {
                db.update_workspace_status(&workspace_id, "ready")?;
                db.update_workspace_stats(
                    &workspace_id,
                    result.symbols_total as i64,
                    result.files_total as i64,
                    None,
                    None,
                    Some(result.duration_ms),
                )?;

                let embed_count =
                    crate::tools::workspace::indexing::embeddings::spawn_workspace_embedding(
                        handler,
                        workspace_id.clone(),
                    )
                    .await;

                let mut message = format!(
                    "Workspace Registered\n\
                     Workspace ID: {}\n\
                     Display Name: {}\n\
                     Path: {}\n\
                     {} files, {} symbols, {} relationships indexed\n\
                     Use manage_workspace(operation=\"open\", workspace_id=\"{}\") to make it current in this session.",
                    workspace_id,
                    display_name,
                    canonical_path_str,
                    result.files_total,
                    result.symbols_total,
                    result.relationships_total,
                    workspace_id,
                );
                if embed_count > 0 {
                    message.push_str(&format!(
                        "\nEmbedding {} symbols in background...",
                        embed_count
                    ));
                }
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
            Err(e) => {
                if let Err(update_err) = db.update_workspace_status(&workspace_id, "error") {
                    warn!(
                        workspace_id = %workspace_id,
                        error = %update_err,
                        "Failed to update workspace status after register error"
                    );
                }
                let message = format!(
                    "Workspace Registration Failed\n\
                     Workspace ID: {}\n\
                     Display Name: {}\n\
                     Path: {}\n\
                     Error: {}",
                    workspace_id, display_name, canonical_path_str, e,
                );
                Ok(CallToolResult::text_content(vec![Content::text(message)]))
            }
        }
    }

    /// Handle remove command - remove workspace by ID and clean up index data.
    pub(crate) async fn handle_remove_command(
        &self,
        handler: &JulieServerHandler,
        workspace_id: &str,
    ) -> Result<CallToolResult> {
        info!("Removing workspace: {}", workspace_id);

        // Daemon mode: use DaemonDatabase
        if let Some(ref db) = handler.daemon_db {
            return Ok(
                match delete_workspace_if_allowed(
                    db,
                    handler.workspace_pool.as_ref(),
                    handler.watcher_pool.as_ref(),
                    workspace_id,
                    CLEANUP_ACTION_MANUAL_DELETE,
                    CLEANUP_REASON_USER_REQUEST,
                )
                .await?
                {
                    WorkspaceDeleteOutcome::Deleted { workspace_id, path } => {
                        CallToolResult::text_content(vec![Content::text(format!(
                            "Workspace Removed Successfully\nWorkspace: {}\nPath: {}\nAll associated index data removed.",
                            workspace_id, path
                        ))])
                    }
                    WorkspaceDeleteOutcome::Blocked {
                        workspace_id,
                        path,
                        reason,
                    } => CallToolResult::text_content(vec![Content::text(format!(
                        "Workspace Delete Blocked\nWorkspace: {}\nPath: {}\nReason: {}",
                        workspace_id, path, reason
                    ))]),
                    WorkspaceDeleteOutcome::NotFound { workspace_id } => {
                        CallToolResult::text_content(vec![Content::text(format!(
                            "Workspace not found: {}",
                            workspace_id
                        ))])
                    }
                },
            );
        }

        // Stdio mode: workspace registry requires daemon mode
        let message =
            "Workspace removal requires daemon mode. Start the daemon with `julie daemon`.";
        Ok(CallToolResult::error(vec![Content::text(message)]))
    }
}
