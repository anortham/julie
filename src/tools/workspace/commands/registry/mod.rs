pub use super::ManageWorkspaceTool;
pub(crate) use status::CheckoutStatus;

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;

use crate::handler::JulieServerHandler;
use crate::registry::database::DaemonDatabase;
use crate::registry::workspace_registry_store::WorkspaceRegistryStore;

use self::cleanup::WorkspaceCleanupActivity;

pub(crate) fn registry_store_for(
    daemon_db: &Arc<DaemonDatabase>,
) -> Result<WorkspaceRegistryStore> {
    let indexes_dir = crate::paths::RegistryPaths::try_new()?.indexes_dir();

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
    live_workspace_ids.extend(handler.active_workspace_ids().await);
    WorkspaceCleanupActivity::new(live_workspace_ids)
}

pub(crate) mod cleanup;
mod health;
mod list;
mod open;
mod refresh;
mod remove;
pub(crate) mod status;
