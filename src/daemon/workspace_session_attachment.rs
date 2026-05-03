use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};

use anyhow::Result;

use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::session_workspace::SessionWorkspaceState;

#[derive(Clone)]
pub struct WorkspaceSessionAttachment {
    workspace_pool: Option<Arc<WorkspacePool>>,
    session_workspace: Arc<StdRwLock<SessionWorkspaceState>>,
}

impl WorkspaceSessionAttachment {
    pub fn new(
        workspace_pool: Option<Arc<WorkspacePool>>,
        session_workspace: Arc<StdRwLock<SessionWorkspaceState>>,
    ) -> Self {
        Self {
            workspace_pool,
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
        self.attach_workspace_once_inner(workspace_id, workspace_root, false)
            .await
    }

    pub async fn attach_workspace_once_and_sync_indexed(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<bool> {
        self.attach_workspace_once_inner(workspace_id, workspace_root, true)
            .await
    }

    async fn attach_workspace_once_inner(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
        sync_indexed: bool,
    ) -> Result<bool> {
        if self.was_attached(workspace_id) {
            return Ok(false);
        }

        let Some(pool) = &self.workspace_pool else {
            return Ok(false);
        };

        pool.get_or_init(workspace_id, workspace_root).await?;
        if sync_indexed {
            pool.sync_indexed_from_db(workspace_id).await;
        }

        Ok(self.mark_workspace_attached(workspace_id.to_string()))
    }
}
