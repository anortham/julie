// Workspace registry command implementations split into focused modules for maintainability.
// Each module contains related command handlers with <= 500 line limit per CLAUDE.md
//
// Module breakdown:
// - register_remove: workspace registration and deletion
// - cleanup: shared prune logic for manual and automatic workspace cleanup
// - list_clean: workspace listing and cleanup operations
// - refresh_stats: workspace re-indexing and statistics
// - health: comprehensive system health checks

pub use super::ManageWorkspaceTool;

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_registry_store::WorkspaceRegistryStore;
use crate::handler::JulieServerHandler;

use self::cleanup::WorkspaceCleanupActivity;

pub(crate) fn registry_store_for(
    daemon_db: &Arc<DaemonDatabase>,
) -> Result<WorkspaceRegistryStore> {
    let indexes_dir = crate::paths::DaemonPaths::try_new()?.indexes_dir();

    Ok(WorkspaceRegistryStore::new(
        Arc::clone(daemon_db),
        indexes_dir,
    ))
}

pub(crate) fn registry_store_for_handler(
    handler: &JulieServerHandler,
) -> Result<Option<WorkspaceRegistryStore>> {
    let Some(daemon_db) = handler.daemon_db.as_ref() else {
        return Ok(None);
    };

    Ok(Some(registry_store_for(daemon_db)?))
}

pub(crate) async fn cleanup_activity_for_handler(
    handler: &JulieServerHandler,
) -> WorkspaceCleanupActivity {
    let mut live_workspace_ids = HashSet::new();
    if let Some(workspace_id) = handler.current_workspace_id() {
        live_workspace_ids.insert(workspace_id);
    }
    live_workspace_ids.extend(handler.session_attached_workspace_ids().await);
    WorkspaceCleanupActivity::new(live_workspace_ids)
}

// Split command implementations into logical modules
pub(crate) mod cleanup;
mod health;
mod list_clean;
mod open;
mod refresh_stats;
mod register_remove;
