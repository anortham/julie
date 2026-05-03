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

use std::sync::Arc;

use anyhow::Result;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::daemon::workspace_registry_store::WorkspaceRegistryStore;

pub(crate) fn registry_store_for(
    daemon_db: &Arc<DaemonDatabase>,
    workspace_pool: Option<&Arc<WorkspacePool>>,
) -> Result<WorkspaceRegistryStore> {
    let indexes_dir = if let Some(pool) = workspace_pool {
        pool.indexes_dir().to_path_buf()
    } else {
        crate::paths::DaemonPaths::try_new()?.indexes_dir()
    };

    Ok(WorkspaceRegistryStore::new(
        Arc::clone(daemon_db),
        indexes_dir,
    ))
}

// Split command implementations into logical modules
pub(crate) mod cleanup;
mod health;
mod list_clean;
mod open;
mod refresh_stats;
mod register_remove;
