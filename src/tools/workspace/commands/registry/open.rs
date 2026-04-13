use std::path::PathBuf;

use anyhow::{Result, anyhow};
use tracing::info;

use super::ManageWorkspaceTool;
use super::refresh_stats::RefreshWorkspaceOutcome;
use crate::handler::JulieServerHandler;
use crate::mcp_compat::{CallToolResult, CallToolResultExt, Content};
use crate::workspace::registry::generate_workspace_id;

struct OpenTarget {
    workspace_id: String,
    workspace_path: PathBuf,
    canonical_path: String,
    is_primary: bool,
    status: String,
}

impl ManageWorkspaceTool {
    fn opened_message(target: &OpenTarget) -> CallToolResult {
        let message = format!(
            "Workspace Opened\nWorkspace ID: {}\nPath: {}",
            target.workspace_id, target.canonical_path,
        );
        CallToolResult::text_content(vec![Content::text(message)])
    }

    pub(crate) async fn handle_open_command(
        &self,
        handler: &JulieServerHandler,
    ) -> Result<CallToolResult> {
        let Some(db) = handler.daemon_db.as_ref() else {
            let message =
                "Workspace open requires daemon mode. Start the daemon with `julie daemon`.";
            return Ok(CallToolResult::text_content(vec![Content::text(message)]));
        };

        // A primary workspace swap is already in progress; refuse to mutate
        // session state or primary binding concurrently. The swap machinery
        // holds this flag only briefly, so retry is the right remedy.
        if handler.is_primary_workspace_swap_in_progress() {
            return Err(anyhow!(
                "Primary workspace swap in progress; retry 'open' after the swap completes."
            ));
        }

        let current_primary_id = handler.current_workspace_id();

        let target = if let Some(path) = self.path.as_ref() {
            if self.workspace_id.is_some() {
                return Err(anyhow!(
                    "Provide either 'path' or 'workspace_id' for 'open', not both"
                ));
            }

            let expanded = shellexpand::tilde(path).to_string();
            let canonical_path = PathBuf::from(expanded)
                .canonicalize()
                .map_err(|e| anyhow!("Failed to canonicalize workspace path '{}': {e}", path))?;
            let canonical_path_str = canonical_path.to_string_lossy().to_string();

            if let Some(row) = db.get_workspace_by_path(&canonical_path_str)? {
                let workspace_id = row.workspace_id;
                let status = row.status;
                let is_primary = current_primary_id.as_deref() == Some(workspace_id.as_str());
                OpenTarget {
                    is_primary,
                    workspace_id,
                    workspace_path: canonical_path,
                    canonical_path: canonical_path_str,
                    status,
                }
            } else {
                let workspace_id = generate_workspace_id(&canonical_path_str)?;
                db.upsert_workspace(&workspace_id, &canonical_path_str, "pending")?;
                OpenTarget {
                    is_primary: current_primary_id.as_deref() == Some(workspace_id.as_str()),
                    workspace_id,
                    workspace_path: canonical_path,
                    canonical_path: canonical_path_str,
                    status: "pending".to_string(),
                }
            }
        } else if let Some(workspace_id) = self.workspace_id.as_ref() {
            let row = db
                .get_workspace(workspace_id)?
                .ok_or_else(|| anyhow!("Workspace not found: {workspace_id}"))?;
            let workspace_id = row.workspace_id;
            let row_path = row.path;
            let status = row.status;
            OpenTarget {
                is_primary: current_primary_id.as_deref() == Some(workspace_id.as_str()),
                workspace_id,
                workspace_path: PathBuf::from(&row_path),
                canonical_path: row_path,
                status,
            }
        } else {
            return Err(anyhow!(
                "'path' or 'workspace_id' parameter required for 'open' operation"
            ));
        };

        info!(
            workspace_id = %target.workspace_id,
            path = %target.canonical_path,
            "Opening workspace for current session"
        );

        let force = self.force.unwrap_or(false);
        let already_active = handler.is_workspace_active(&target.workspace_id).await;
        let attached_matches_target = handler
            .was_workspace_attached_in_session(&target.workspace_id)
            .await;
        if already_active && !force && (!target.is_primary || attached_matches_target) {
            return Ok(Self::opened_message(&target));
        }

        if target.status != "ready" {
            let result = self
                .handle_index_command(handler, Some(target.canonical_path.clone()), force, false)
                .await?;

            let indexed_ready = handler
                .daemon_db
                .as_ref()
                .and_then(|db| db.get_workspace(&target.workspace_id).ok().flatten())
                .is_some_and(|row| row.status == "ready");
            if !indexed_ready {
                return Ok(result);
            }
        } else if !target.is_primary {
            match self
                .refresh_workspace_internal(handler, &target.workspace_id)
                .await?
            {
                RefreshWorkspaceOutcome::Success(_) => {}
                RefreshWorkspaceOutcome::Failure(message) => {
                    return Ok(CallToolResult::text_content(vec![Content::text(message)]));
                }
            }
        }

        handler
            .activate_workspace_with_root(&target.workspace_id, target.workspace_path.clone())
            .await?;

        Ok(Self::opened_message(&target))
    }
}
