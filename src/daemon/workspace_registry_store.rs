use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use crate::daemon::database::{DaemonDatabase, WorkspaceRow};

#[derive(Clone)]
pub struct WorkspaceRegistryStore {
    daemon_db: Arc<DaemonDatabase>,
    indexes_dir: PathBuf,
}

impl WorkspaceRegistryStore {
    pub fn new(daemon_db: Arc<DaemonDatabase>, indexes_dir: PathBuf) -> Self {
        Self {
            daemon_db,
            indexes_dir,
        }
    }

    pub fn indexes_dir(&self) -> &Path {
        &self.indexes_dir
    }

    pub fn index_dir_for(&self, workspace_id: &str) -> PathBuf {
        self.indexes_dir.join(workspace_id)
    }

    pub fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRow>> {
        self.daemon_db.get_workspace(workspace_id)
    }

    pub fn get_workspace_by_path(&self, path: &str) -> Result<Option<WorkspaceRow>> {
        self.daemon_db.get_workspace_by_path(path)
    }

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceRow>> {
        self.daemon_db.list_workspaces()
    }

    pub fn upsert_workspace(&self, workspace_id: &str, path: &str, status: &str) -> Result<()> {
        self.daemon_db.upsert_workspace(workspace_id, path, status)
    }

    pub fn update_workspace_status(&self, workspace_id: &str, status: &str) -> Result<()> {
        self.daemon_db.update_workspace_status(workspace_id, status)
    }

    pub fn update_workspace_stats(
        &self,
        workspace_id: &str,
        symbol_count: i64,
        file_count: i64,
        embedding_model: Option<&str>,
        vector_count: Option<i64>,
        index_duration_ms: Option<u64>,
    ) -> Result<()> {
        self.daemon_db.update_workspace_stats(
            workspace_id,
            symbol_count,
            file_count,
            embedding_model,
            vector_count,
            index_duration_ms,
        )
    }

    pub fn record_cleanup_event(
        &self,
        workspace_id: &str,
        path: &str,
        action: &str,
        reason: &str,
    ) -> Result<()> {
        self.daemon_db
            .insert_cleanup_event(workspace_id, path, action, reason)
    }

    pub fn delete_workspace_and_record_cleanup(
        &self,
        workspace: &WorkspaceRow,
        action: &str,
        reason: &str,
    ) -> Result<()> {
        self.daemon_db.delete_workspace(&workspace.workspace_id)?;
        self.record_cleanup_event(&workspace.workspace_id, &workspace.path, action, reason)
    }
}
