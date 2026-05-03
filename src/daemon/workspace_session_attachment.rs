use std::path::PathBuf;
use std::sync::{Arc, RwLock as StdRwLock};

use anyhow::Result;
use tracing::warn;

use crate::daemon::database::DaemonDatabase;
use crate::daemon::embedding_service::EmbeddingService;
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;
use crate::handler::session_workspace::SessionWorkspaceState;

#[derive(Clone)]
pub struct WorkspaceSessionAttachment {
    workspace_pool: Option<Arc<WorkspacePool>>,
    daemon_db: Option<Arc<DaemonDatabase>>,
    watcher_pool: Option<Arc<WatcherPool>>,
    embedding_service: Option<Arc<EmbeddingService>>,
    session_workspace: Arc<StdRwLock<SessionWorkspaceState>>,
}

impl WorkspaceSessionAttachment {
    pub fn new(
        workspace_pool: Option<Arc<WorkspacePool>>,
        daemon_db: Option<Arc<DaemonDatabase>>,
        watcher_pool: Option<Arc<WatcherPool>>,
        embedding_service: Option<Arc<EmbeddingService>>,
        session_workspace: Arc<StdRwLock<SessionWorkspaceState>>,
    ) -> Self {
        Self {
            workspace_pool,
            daemon_db,
            watcher_pool,
            embedding_service,
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

        self.attach_workspace_resources(workspace_id, workspace_root)
            .await?;
        if sync_indexed {
            if let Some(pool) = &self.workspace_pool {
                pool.sync_indexed_from_db(workspace_id).await;
            }
        }

        Ok(self.mark_workspace_attached(workspace_id.to_string()))
    }

    pub async fn attach_workspace_resources(
        &self,
        workspace_id: &str,
        workspace_root: PathBuf,
    ) -> Result<()> {
        let Some(pool) = &self.workspace_pool else {
            return Ok(());
        };

        let workspace = pool.get_or_init(workspace_id, workspace_root).await?;
        self.update_session_count(workspace_id, true).await;
        if let Some(watcher_pool) = &self.watcher_pool {
            let provider = self
                .embedding_service
                .as_ref()
                .and_then(|service| service.provider());
            if let Err(error) = watcher_pool
                .attach(workspace_id, &workspace, provider)
                .await
            {
                warn!(
                    workspace_id,
                    "Failed to attach watcher during session attachment: {error}"
                );
            }
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
        if let Some(watcher_pool) = &self.watcher_pool {
            watcher_pool.detach(workspace_id).await;
        }
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
