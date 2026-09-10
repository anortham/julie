use std::path::PathBuf;

use crate::dashboard::AppState;
use crate::handler::JulieServerHandler;
use crate::workspace::registry::generate_workspace_id;
use crate::workspace::startup_hint::{WorkspaceStartupHint, WorkspaceStartupSource};

pub(crate) async fn dashboard_handler(
    state: &AppState,
) -> anyhow::Result<(JulieServerHandler, tempfile::TempDir, String)> {
    let anchor_dir = tempfile::tempdir()?;
    let anchor_path = anchor_dir.path().to_path_buf();
    let anchor_id = generate_workspace_id(&anchor_path.to_string_lossy())?;
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let handler = JulieServerHandler::new_deferred_daemon_startup_hint_without_project_log(
        WorkspaceStartupHint {
            path: workspace_root,
            source: Some(WorkspaceStartupSource::Cwd),
        },
        state.dashboard.daemon_db().cloned(),
        Some(state.dashboard.sender()),
    )
    .await?;

    handler
        .initialize_workspace_with_force(Some(anchor_path.to_string_lossy().to_string()), false)
        .await?;

    Ok((handler, anchor_dir, anchor_id))
}

pub(crate) async fn cleanup_dashboard_anchor(state: &AppState, anchor_id: &str) {
    if let Some(daemon_db) = state.dashboard.daemon_db() {
        let _ = daemon_db.delete_workspace(anchor_id);
    }
}
