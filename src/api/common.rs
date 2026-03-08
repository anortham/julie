//! Shared helpers for API handlers.
//!
//! Common utilities used across multiple API modules (search, memories, agents).

use axum::http::StatusCode;

use crate::daemon_state::{DaemonState, LoadedWorkspace, WorkspaceLoadStatus};

/// Maximum number of results any search/list endpoint will return.
pub const MAX_RESULT_LIMIT: usize = 500;

/// Resolve a Ready workspace from the daemon state.
///
/// If `project` is `Some`, looks up that specific workspace ID and verifies
/// it is Ready. If `None`, returns the first Ready workspace.
///
/// Returns 404 if the workspace is not found, or if the specified workspace
/// exists but is not in Ready status.
pub fn resolve_workspace<'a>(
    daemon_state: &'a DaemonState,
    project: Option<&str>,
) -> Result<&'a LoadedWorkspace, (StatusCode, String)> {
    match project {
        Some(id) => {
            let loaded = daemon_state.workspaces.get(id).ok_or((
                StatusCode::NOT_FOUND,
                format!("Workspace not found: {}", id),
            ))?;
            if loaded.status != WorkspaceLoadStatus::Ready {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "Workspace '{}' is not ready (status: {:?})",
                        id, loaded.status
                    ),
                ));
            }
            Ok(loaded)
        }
        None => {
            // Find the first Ready workspace
            daemon_state
                .workspaces
                .values()
                .find(|ws| ws.status == WorkspaceLoadStatus::Ready)
                .ok_or((
                    StatusCode::NOT_FOUND,
                    "No ready workspace available".to_string(),
                ))
        }
    }
}
