//! Watcher integration methods for `DaemonState`.
//!
//! Extracted from `daemon_state.rs` to keep that file under the 500-line limit.
//! These methods start file watchers for `Ready` projects so the daemon detects
//! file changes and triggers incremental re-indexing.

use tracing::{debug, info};

use crate::daemon_state::{DaemonState, WorkspaceLoadStatus};

impl DaemonState {
    /// Start file watchers for all `Ready` projects.
    ///
    /// Called after `load_registered_projects` on daemon startup.
    /// Only starts watchers for workspaces that have both a database and
    /// search index loaded (status == Ready).
    pub async fn start_watchers_for_ready_projects(&self) {
        let mut started = 0u32;
        for (workspace_id, loaded) in &self.workspaces {
            if loaded.status != WorkspaceLoadStatus::Ready {
                continue;
            }

            let (db, search_index) = match (&loaded.workspace.db, &loaded.workspace.search_index) {
                (Some(db), si) => (db.clone(), si.clone()),
                _ => {
                    debug!(
                        "Skipping watcher for '{}': no database loaded",
                        workspace_id
                    );
                    continue;
                }
            };

            self.watcher_manager
                .start_watching(
                    workspace_id.clone(),
                    loaded.path.clone(),
                    db,
                    search_index,
                )
                .await;
            started += 1;
        }
        info!("Started file watchers for {} Ready project(s)", started);
    }

    /// Start a file watcher for a single workspace if it's Ready.
    ///
    /// Called after registering a new project via the API.
    pub async fn start_watcher_if_ready(&self, workspace_id: &str) {
        let loaded = match self.workspaces.get(workspace_id) {
            Some(lw) => lw,
            None => return,
        };

        if loaded.status != WorkspaceLoadStatus::Ready {
            debug!(
                "Not starting watcher for '{}': status is {:?}",
                workspace_id, loaded.status
            );
            return;
        }

        let (db, search_index) = match (&loaded.workspace.db, &loaded.workspace.search_index) {
            (Some(db), si) => (db.clone(), si.clone()),
            _ => return,
        };

        self.watcher_manager
            .start_watching(
                workspace_id.to_string(),
                loaded.path.clone(),
                db,
                search_index,
            )
            .await;
    }
}
