use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};

use anyhow::Result;
use tracing::warn;

use crate::daemon::database::DaemonDatabase;
use crate::handler::session_workspace::SessionWorkspaceState;

#[derive(Clone)]
pub struct WorkspaceSessionAttachment {
    daemon_db: Option<Arc<DaemonDatabase>>,
    session_workspace: Arc<StdRwLock<SessionWorkspaceState>>,
}

impl WorkspaceSessionAttachment {
    pub fn new(
        daemon_db: Option<Arc<DaemonDatabase>>,
        session_workspace: Arc<StdRwLock<SessionWorkspaceState>>,
    ) -> Self {
        Self {
            daemon_db,
            session_workspace,
        }
    }

    pub fn was_attached(&self, workspace_id: &str) -> bool {
        self.session_workspace
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .was_workspace_attached_in_session(workspace_id)
    }

    pub fn mark_workspace_attached(&self, workspace_id: impl Into<String>) -> bool {
        self.session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .mark_workspace_attached(workspace_id)
    }

    pub async fn attach_workspace_once(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<bool> {
        if self.was_attached(workspace_id) {
            return Ok(false);
        }

        self.attach_workspace_resources(workspace_id, workspace_root)
            .await?;

        Ok(self.mark_workspace_attached(workspace_id.to_string()))
    }

    pub async fn attach_workspace_resources(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<()> {
        // In-process mode: register the workspace in daemon_db so it is
        // visible to `manage_workspace list` and reachable via workspace-ID
        // tool parameters (resolve_workspace_filter). No session-count
        // tracking — that was a daemon-pool concept.
        //
        // Intentionally synchronous (no spawn_blocking): `upsert_workspace`
        // is a fast SQLite write (<1 ms). Keeping it synchronous ensures no
        // yield occurs between this call and `apply_root_snapshot` in
        // `reconcile_primary_workspace_roots`, preventing a race where a
        // concurrent `list_roots_from_peer` call observes an unbound state.
        if let Some(db) = self.daemon_db.as_deref() {
            let workspace_root_str = workspace_root.to_string_lossy();
            db.upsert_workspace(workspace_id, &workspace_root_str, "ready")?;
        }
        Ok(())
    }

    pub async fn detach_workspace_once(&self, workspace_id: &str) -> Result<bool> {
        let was_attached = self
            .session_workspace
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .mark_workspace_detached(workspace_id);
        if !was_attached {
            return Ok(false);
        }

        self.detach_workspace_resources(workspace_id).await?;
        Ok(true)
    }

    pub async fn detach_workspace_resources(&self, workspace_id: &str) -> Result<()> {
        self.update_session_count(workspace_id, false).await;
        Ok(())
    }

    async fn update_session_count(&self, workspace_id: &str, increment: bool) {
        let Some(db) = self.daemon_db.as_ref().map(Arc::clone) else {
            return;
        };
        let workspace_id = workspace_id.to_string();
        let workspace_id_for_log = workspace_id.clone();
        let op = if increment { "increment" } else { "decrement" };

        let result = tokio::task::spawn_blocking(move || {
            if increment {
                db.increment_session_count(&workspace_id)
            } else {
                db.decrement_session_count(&workspace_id)
            }
        })
        .await;

        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!(
                    workspace_id = workspace_id_for_log,
                    "Failed to {op} workspace session count in daemon.db: {error}"
                );
            }
            Err(error) => {
                warn!(
                    workspace_id = workspace_id_for_log,
                    "Session count {op} task failed: {error}"
                );
            }
        }
    }
}
