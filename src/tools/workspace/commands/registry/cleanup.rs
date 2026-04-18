use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn};

use crate::daemon::database::{DaemonDatabase, WorkspaceRow};
use crate::daemon::watcher_pool::WatcherPool;
use crate::daemon::workspace_pool::WorkspacePool;

pub(crate) const CLEANUP_ACTION_AUTO_PRUNE: &str = "auto_prune";
pub(crate) const CLEANUP_ACTION_MANUAL_DELETE: &str = "manual_delete";
pub(crate) const CLEANUP_REASON_MISSING_PATH: &str = "missing_path";
pub(crate) const CLEANUP_REASON_ORPHAN_INDEX_DIR: &str = "orphan_index_dir";
pub(crate) const CLEANUP_REASON_USER_REQUEST: &str = "user_request";
pub(crate) const MISSING_PATH_RECHECK_DELAY: Duration = Duration::from_millis(250);

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

pub(crate) async fn path_missing_after_grace(path: &Path, recheck_delay: Duration) -> Result<bool> {
    if path.try_exists()? {
        return Ok(false);
    }

    tokio::time::sleep(recheck_delay).await;
    Ok(!path.try_exists()?)
}

fn index_dir_for(
    workspace_pool: Option<&Arc<WorkspacePool>>,
    workspace_id: &str,
) -> Result<PathBuf> {
    if let Some(pool) = workspace_pool {
        return Ok(pool.indexes_dir().join(workspace_id));
    }

    Ok(crate::paths::DaemonPaths::try_new()?
        .indexes_dir()
        .join(workspace_id))
}

async fn watcher_ref_count(watcher_pool: Option<&Arc<WatcherPool>>, workspace_id: &str) -> usize {
    if let Some(pool) = watcher_pool {
        pool.ref_count(workspace_id).await
    } else {
        0
    }
}

async fn live_indexing_reason(
    workspace_pool: Option<&Arc<WorkspacePool>>,
    workspace_id: &str,
) -> Option<String> {
    let Some(pool) = workspace_pool else {
        return None;
    };
    let snapshot = pool.indexing_snapshot(workspace_id).await?;
    if let Some(operation) = snapshot.active_operation {
        return Some(format!(
            "indexing operation '{}' is still running",
            operation
        ));
    }
    if snapshot.catchup_active {
        return Some("catch-up indexing is still running".to_string());
    }
    None
}

async fn manual_delete_block_reason(
    workspace: &WorkspaceRow,
    workspace_pool: Option<&Arc<WorkspacePool>>,
    watcher_pool: Option<&Arc<WatcherPool>>,
) -> Option<String> {
    let mut reasons = Vec::new();

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

    let watcher_refs = watcher_ref_count(watcher_pool, &workspace.workspace_id).await;
    if watcher_refs > 0 {
        let suffix = if watcher_refs == 1 { "" } else { "s" };
        reasons.push(format!("{watcher_refs} watcher ref{suffix} remain"));
    }

    if workspace.status == "indexing" {
        reasons.push("workspace status is indexing".to_string());
    }

    if let Some(reason) = live_indexing_reason(workspace_pool, &workspace.workspace_id).await {
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
    workspace_pool: Option<&Arc<WorkspacePool>>,
    watcher_pool: Option<&Arc<WatcherPool>>,
) -> Option<String> {
    if workspace.session_count > 0 {
        return Some(format!(
            "{} active session(s) remain",
            workspace.session_count
        ));
    }

    let watcher_refs = watcher_ref_count(watcher_pool, &workspace.workspace_id).await;
    if watcher_refs > 0 {
        return Some(format!("{watcher_refs} watcher ref(s) remain"));
    }

    live_indexing_reason(workspace_pool, &workspace.workspace_id).await
}

pub(crate) async fn delete_workspace_if_allowed(
    daemon_db: &Arc<DaemonDatabase>,
    workspace_pool: Option<&Arc<WorkspacePool>>,
    watcher_pool: Option<&Arc<WatcherPool>>,
    workspace_id: &str,
    action: &str,
    reason: &str,
) -> Result<WorkspaceDeleteOutcome> {
    let Some(workspace) = daemon_db.get_workspace(workspace_id)? else {
        return Ok(WorkspaceDeleteOutcome::NotFound {
            workspace_id: workspace_id.to_string(),
        });
    };

    let block_reason = if action == CLEANUP_ACTION_MANUAL_DELETE {
        manual_delete_block_reason(&workspace, workspace_pool, watcher_pool).await
    } else {
        auto_prune_block_reason(&workspace, workspace_pool, watcher_pool).await
    };
    if let Some(reason) = block_reason {
        return Ok(WorkspaceDeleteOutcome::Blocked {
            workspace_id: workspace.workspace_id.clone(),
            path: workspace.path.clone(),
            reason,
        });
    }

    if let Some(pool) = watcher_pool {
        let removed = pool.remove_if_inactive(&workspace.workspace_id).await?;
        if !removed {
            return Ok(WorkspaceDeleteOutcome::Blocked {
                workspace_id: workspace.workspace_id.clone(),
                path: workspace.path.clone(),
                reason: "watcher is still attached".to_string(),
            });
        }
    }

    if let Some(pool) = workspace_pool {
        pool.evict_workspace(&workspace.workspace_id).await;
    }

    let index_dir = index_dir_for(workspace_pool, &workspace.workspace_id)?;
    if index_dir.exists() {
        tokio::fs::remove_dir_all(&index_dir).await?;
    }

    daemon_db.delete_workspace(&workspace.workspace_id)?;
    daemon_db.insert_cleanup_event(&workspace.workspace_id, &workspace.path, action, reason)?;
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
    daemon_db: &Arc<DaemonDatabase>,
    workspace_pool: Option<&Arc<WorkspacePool>>,
    watcher_pool: Option<&Arc<WatcherPool>>,
) -> Result<CleanupSweepSummary> {
    let all_workspaces = daemon_db.list_workspaces()?;
    let mut summary = CleanupSweepSummary::default();

    for workspace in all_workspaces {
        match path_missing_after_grace(Path::new(&workspace.path), MISSING_PATH_RECHECK_DELAY).await
        {
            Ok(true) => {}
            Ok(false) => continue,
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
            daemon_db,
            workspace_pool,
            watcher_pool,
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
    daemon_db: &Arc<DaemonDatabase>,
    workspace_pool: Option<&Arc<WorkspacePool>>,
) -> Result<Vec<String>> {
    let indexes_dir = if let Some(pool) = workspace_pool {
        pool.indexes_dir().to_path_buf()
    } else {
        crate::paths::DaemonPaths::try_new()?.indexes_dir()
    };

    let registered_ids: HashSet<String> = daemon_db
        .list_workspaces()?
        .into_iter()
        .map(|workspace| workspace.workspace_id)
        .collect();

    let mut removed = Vec::new();
    let entries = match std::fs::read_dir(&indexes_dir) {
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
        daemon_db.insert_cleanup_event(
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
    daemon_db: &Arc<DaemonDatabase>,
    workspace_pool: Option<&Arc<WorkspacePool>>,
    watcher_pool: Option<&Arc<WatcherPool>>,
) -> Result<CleanupSweepSummary> {
    let mut summary = prune_missing_workspaces(daemon_db, workspace_pool, watcher_pool).await?;
    summary.pruned_orphan_dirs = prune_orphan_index_dirs(daemon_db, workspace_pool).await?;
    Ok(summary)
}
