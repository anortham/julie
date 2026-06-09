use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn};

use crate::registry::database::WorkspaceRow;
use crate::registry::workspace_registry_store::WorkspaceRegistryStore;

pub(crate) const CLEANUP_ACTION_AUTO_PRUNE: &str = "auto_prune";
pub(crate) const CLEANUP_ACTION_MANUAL_DELETE: &str = "manual_delete";
pub(crate) const CLEANUP_REASON_MISSING_PATH: &str = "missing_path";
pub(crate) const CLEANUP_REASON_ORPHAN_INDEX_DIR: &str = "orphan_index_dir";
pub(crate) const CLEANUP_REASON_USER_REQUEST: &str = "user_request";
pub(crate) const MISSING_PATH_RECHECK_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceCleanupState {
    Present,
    MissingPrunable,
    MissingBlocked { reason: String },
}

pub(crate) enum WorkspaceDeleteOutcome {
    Deleted {
        workspace_id: String,
        path: String,
    },
    Blocked {
        workspace_id: String,
        path: String,
        reason: String,
    },
    NotFound {
        workspace_id: String,
    },
}

#[derive(Default)]
pub(crate) struct CleanupSweepSummary {
    pub(crate) pruned_workspaces: Vec<String>,
    pub(crate) blocked_workspaces: Vec<(String, String)>,
    pub(crate) pruned_orphan_dirs: Vec<String>,
}

pub(crate) struct WorkspaceCleanupActivity {
    live_workspace_ids: HashSet<String>,
}

impl WorkspaceCleanupActivity {
    pub(crate) fn new(live_workspace_ids: HashSet<String>) -> Self {
        Self { live_workspace_ids }
    }

    fn live_workspace_reason(&self, workspace_id: &str) -> Option<String> {
        self.live_workspace_ids
            .contains(workspace_id)
            .then(|| "workspace is active in this in-process session".to_string())
    }

    async fn watcher_ref_count(&self, _workspace_id: &str) -> usize {
        // In-process mode: no shared watcher pool; no active refs.
        0
    }

    async fn live_indexing_reason(&self, _workspace_id: &str) -> Option<String> {
        // In-process mode: no shared workspace pool to query.
        None
    }

    async fn remove_runtime_if_inactive(&self, _workspace_id: &str) -> Result<bool> {
        // In-process mode: no pool entries to evict.
        Ok(true)
    }
}

pub(crate) async fn path_missing_after_grace(path: &Path, recheck_delay: Duration) -> Result<bool> {
    if path.try_exists()? {
        return Ok(false);
    }

    tokio::time::sleep(recheck_delay).await;
    Ok(!path.try_exists()?)
}

async fn manual_delete_block_reason(
    workspace: &WorkspaceRow,
    activity: &WorkspaceCleanupActivity,
) -> Option<String> {
    let mut reasons = Vec::new();

    if let Some(reason) = activity.live_workspace_reason(&workspace.workspace_id) {
        reasons.push(reason);
    }

    if workspace.session_count > 0 {
        let suffix = if workspace.session_count == 1 {
            ""
        } else {
            "s"
        };
        reasons.push(format!(
            "{} active session{} remain",
            workspace.session_count, suffix
        ));
    }

    let watcher_refs = activity.watcher_ref_count(&workspace.workspace_id).await;
    if watcher_refs > 0 {
        let suffix = if watcher_refs == 1 { "" } else { "s" };
        reasons.push(format!("{watcher_refs} watcher ref{suffix} remain"));
    }

    if workspace.status == "indexing" {
        reasons.push("workspace status is indexing".to_string());
    }

    if let Some(reason) = activity.live_indexing_reason(&workspace.workspace_id).await {
        reasons.push(reason);
    }

    if reasons.is_empty() {
        None
    } else {
        Some(reasons.join(", "))
    }
}

async fn auto_prune_block_reason(
    workspace: &WorkspaceRow,
    activity: &WorkspaceCleanupActivity,
) -> Option<String> {
    if let Some(reason) = activity.live_workspace_reason(&workspace.workspace_id) {
        return Some(reason);
    }

    if workspace.session_count > 0 {
        return Some(format!(
            "{} active session(s) remain",
            workspace.session_count
        ));
    }

    let watcher_refs = activity.watcher_ref_count(&workspace.workspace_id).await;
    if watcher_refs > 0 {
        return Some(format!("{watcher_refs} watcher ref(s) remain"));
    }

    activity.live_indexing_reason(&workspace.workspace_id).await
}

async fn cleanup_block_reason(
    workspace: &WorkspaceRow,
    activity: &WorkspaceCleanupActivity,
    action: &str,
) -> Option<String> {
    if action == CLEANUP_ACTION_MANUAL_DELETE {
        manual_delete_block_reason(workspace, activity).await
    } else {
        auto_prune_block_reason(workspace, activity).await
    }
}

pub(crate) async fn inspect_workspace_cleanup_state(
    workspace: &WorkspaceRow,
    activity: &WorkspaceCleanupActivity,
    action: &str,
) -> Result<WorkspaceCleanupState> {
    if !path_missing_after_grace(Path::new(&workspace.path), MISSING_PATH_RECHECK_DELAY).await? {
        return Ok(WorkspaceCleanupState::Present);
    }

    if let Some(reason) = cleanup_block_reason(workspace, activity, action).await {
        return Ok(WorkspaceCleanupState::MissingBlocked { reason });
    }

    Ok(WorkspaceCleanupState::MissingPrunable)
}

pub(crate) async fn delete_workspace_if_allowed(
    registry_store: &WorkspaceRegistryStore,
    activity: &WorkspaceCleanupActivity,
    workspace_id: &str,
    action: &str,
    reason: &str,
) -> Result<WorkspaceDeleteOutcome> {
    let Some(workspace) = registry_store.get_workspace(workspace_id)? else {
        return Ok(WorkspaceDeleteOutcome::NotFound {
            workspace_id: workspace_id.to_string(),
        });
    };

    let block_reason = cleanup_block_reason(&workspace, activity, action).await;
    if let Some(reason) = block_reason {
        return Ok(WorkspaceDeleteOutcome::Blocked {
            workspace_id: workspace.workspace_id.clone(),
            path: workspace.path.clone(),
            reason,
        });
    }

    if !activity
        .remove_runtime_if_inactive(&workspace.workspace_id)
        .await?
    {
        return Ok(WorkspaceDeleteOutcome::Blocked {
            workspace_id: workspace.workspace_id.clone(),
            path: workspace.path.clone(),
            reason: "watcher is still attached".to_string(),
        });
    }

    let index_dir = registry_store.index_dir_for(&workspace.workspace_id);
    if index_dir.exists() {
        tokio::fs::remove_dir_all(&index_dir).await?;
    }

    registry_store.delete_workspace_and_record_cleanup(&workspace, action, reason)?;
    info!(
        workspace_id = %workspace.workspace_id,
        path = %workspace.path,
        action,
        reason,
        "Removed workspace during cleanup"
    );

    Ok(WorkspaceDeleteOutcome::Deleted {
        workspace_id: workspace.workspace_id,
        path: workspace.path,
    })
}

pub(crate) async fn prune_missing_workspaces(
    registry_store: &WorkspaceRegistryStore,
    activity: &WorkspaceCleanupActivity,
) -> Result<CleanupSweepSummary> {
    let all_workspaces = registry_store.list_workspaces()?;
    let mut summary = CleanupSweepSummary::default();

    for workspace in all_workspaces {
        match inspect_workspace_cleanup_state(&workspace, activity, CLEANUP_ACTION_AUTO_PRUNE).await
        {
            Ok(WorkspaceCleanupState::Present) => continue,
            Ok(WorkspaceCleanupState::MissingBlocked { reason }) => {
                summary
                    .blocked_workspaces
                    .push((workspace.workspace_id.clone(), reason));
                continue;
            }
            Ok(WorkspaceCleanupState::MissingPrunable) => {}
            Err(error) => {
                warn!(
                    workspace_id = %workspace.workspace_id,
                    path = %workspace.path,
                    error = %error,
                    "Skipping auto-prune because workspace path check failed"
                );
                summary.blocked_workspaces.push((
                    workspace.workspace_id.clone(),
                    format!("path check failed: {error}"),
                ));
                continue;
            }
        }

        match delete_workspace_if_allowed(
            registry_store,
            activity,
            &workspace.workspace_id,
            CLEANUP_ACTION_AUTO_PRUNE,
            CLEANUP_REASON_MISSING_PATH,
        )
        .await?
        {
            WorkspaceDeleteOutcome::Deleted { workspace_id, .. } => {
                summary.pruned_workspaces.push(workspace_id);
            }
            WorkspaceDeleteOutcome::Blocked {
                workspace_id,
                reason,
                ..
            } => {
                summary.blocked_workspaces.push((workspace_id, reason));
            }
            WorkspaceDeleteOutcome::NotFound { .. } => {}
        }
    }

    Ok(summary)
}

pub(crate) async fn prune_orphan_index_dirs(
    registry_store: &WorkspaceRegistryStore,
) -> Result<Vec<String>> {
    let registered_ids: HashSet<String> = registry_store
        .list_workspaces()?
        .into_iter()
        .map(|workspace| workspace.workspace_id)
        .collect();

    let mut removed = Vec::new();
    let entries = match std::fs::read_dir(registry_store.indexes_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(removed),
        Err(err) => return Err(err.into()),
    };

    for entry in entries.flatten() {
        if !entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false)
        {
            continue;
        }

        let dir_name = entry.file_name().to_string_lossy().to_string();
        if registered_ids.contains(&dir_name) {
            continue;
        }

        let dir_path = entry.path();
        tokio::fs::remove_dir_all(&dir_path).await?;
        registry_store.record_cleanup_event(
            &dir_name,
            &dir_path.to_string_lossy(),
            CLEANUP_ACTION_AUTO_PRUNE,
            CLEANUP_REASON_ORPHAN_INDEX_DIR,
        )?;
        removed.push(dir_name.clone());
        info!(
            workspace_id = %dir_name,
            path = %dir_path.display(),
            "Removed orphan index directory"
        );
    }

    Ok(removed)
}

pub(crate) async fn run_cleanup_sweep(
    registry_store: &WorkspaceRegistryStore,
    activity: &WorkspaceCleanupActivity,
) -> Result<CleanupSweepSummary> {
    let mut summary = prune_missing_workspaces(registry_store, activity).await?;
    summary.pruned_orphan_dirs = prune_orphan_index_dirs(registry_store).await?;
    Ok(summary)
}
