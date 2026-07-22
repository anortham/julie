//! Workspace-parameter resolution — handler-bound backing.
//!
//! `resolve_workspace_filter` backs `ToolContext::resolve_workspace_target`.
//! It lives here (adjacent to `tool_context_impl.rs`) because it accesses
//! `JulieServerHandler` fields directly: `daemon_db`, `activate_workspace_with_root`,
//! `was_workspace_attached_in_session`, `is_workspace_active`.
//!
//! Handler-free helpers (`parse_qualified_name`, `definition_priority`, etc.)
//! stay in `src/tools/navigation/resolution.rs`.

use std::path::PathBuf;

use anyhow::Result;

use crate::handler::JulieServerHandler;
use julie_context::WorkspaceTarget;
use julie_core::workspace_errors::{WorkspaceResolutionFailure, WorkspaceResolutionFailureKind};

fn workspace_resolution_failure(
    kind: WorkspaceResolutionFailureKind,
    message: impl Into<String>,
) -> anyhow::Error {
    WorkspaceResolutionFailure::new(kind, message).into()
}

/// Given an invalid workspace ID and a list of known workspace IDs,
/// return an error with a fuzzy match suggestion (if one is close enough)
/// or a generic "not found" error.
fn suggest_closest_workspace(workspace_id: &str, known_ids: &[&str]) -> Result<WorkspaceTarget> {
    if let Some((best_match, distance)) =
        crate::utils::string_similarity::find_closest_match(workspace_id, known_ids)
    {
        // Only suggest if the distance is reasonable (< 50% of query length)
        if distance < workspace_id.len() / 2 {
            return Err(workspace_resolution_failure(
                WorkspaceResolutionFailureKind::UnknownWorkspace,
                format!(
                    "Workspace '{}' not found. Did you mean '{}'?",
                    workspace_id, best_match
                ),
            ));
        }
    }

    // No close match found
    Err(workspace_resolution_failure(
        WorkspaceResolutionFailureKind::UnknownWorkspace,
        format!(
            "Workspace '{}' not found. Use 'primary' or a valid workspace ID",
            workspace_id
        ),
    ))
}

/// Resolve workspace parameter to a WorkspaceTarget.
///
/// - `None` or `"primary"` → `WorkspaceTarget::Primary`
/// - Any other string in daemon mode → must be a known workspace ID that is active in the current session
/// - Any other string in stdio mode → accepted permissively as `WorkspaceTarget::Target(id)`
///
/// Daemon mode validates against the daemon registry, then enforces the active-session gate.
pub async fn resolve_workspace_filter(
    workspace_param: Option<&str>,
    handler: &JulieServerHandler,
) -> Result<WorkspaceTarget> {
    let workspace_param = workspace_param.unwrap_or("primary");

    match workspace_param {
        "primary" => Ok(WorkspaceTarget::Primary),
        workspace_id => {
            // Daemon mode: validate against DaemonDatabase and suggest closest match
            if let Some(ref db) = handler.daemon_db {
                return match db.get_workspace(workspace_id)? {
                    Some(workspace_row) => {
                        let startup_workspace_loaded_for_session =
                            handler.loaded_workspace_id().as_deref() == Some(workspace_id)
                                && handler
                                    .was_workspace_attached_in_session(workspace_id)
                                    .await;

                        if handler.is_workspace_active(workspace_id).await
                            || startup_workspace_loaded_for_session
                        {
                            Ok(WorkspaceTarget::Target(workspace_id.to_string()))
                        } else if workspace_row.status != "ready" {
                            Err(workspace_resolution_failure(
                                WorkspaceResolutionFailureKind::WorkspaceNotReady,
                                format!(
                                    "Workspace '{}' is known but has status '{}' (not ready). Run manage_workspace(operation=\"open\", workspace_id=\"{}\") first.",
                                    workspace_id, workspace_row.status, workspace_id
                                ),
                            ))
                        } else if handler.is_primary_workspace_swap_in_progress() {
                            Err(workspace_resolution_failure(
                                WorkspaceResolutionFailureKind::PrimarySwapInProgress,
                                "Primary workspace swap in progress; retry workspace-scoped query after the swap completes.",
                            ))
                        } else {
                            let workspace_root = PathBuf::from(&workspace_row.path);
                            match handler
                                .activate_workspace_with_root(workspace_id, workspace_root)
                                .await
                            {
                                Ok(_) => Ok(WorkspaceTarget::Target(workspace_id.to_string())),
                                Err(error) => Err(workspace_resolution_failure(
                                    WorkspaceResolutionFailureKind::AutoActivationFailed,
                                    format!(
                                        "Workspace '{}' is known but auto-activation failed: {}. Run manage_workspace(operation=\"open\", workspace_id=\"{}\") first.",
                                        workspace_id, error, workspace_id
                                    ),
                                )),
                            }
                        }
                    }
                    None => {
                        let all_workspaces = db.list_workspaces().unwrap_or_default();
                        let workspace_ids: Vec<&str> = all_workspaces
                            .iter()
                            .map(|w| w.workspace_id.as_str())
                            .collect();
                        suggest_closest_workspace(workspace_id, &workspace_ids)
                    }
                };
            }

            // Stdio mode: no registry available, accept without validation
            Ok(WorkspaceTarget::Target(workspace_id.to_string()))
        }
    }
}
