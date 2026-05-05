use crate::handler::JulieServerHandler;
use anyhow::Result;
use std::path::Path;
use std::sync::atomic::Ordering;
use tracing::info;

#[derive(Default)]
pub(crate) struct ForceReindexWatcherPause {
    watcher_pool_workspace_ids: Vec<String>,
    local_primary_paused: bool,
}

pub(crate) fn workspace_ids_for_force_reindex(
    canonical_path: &Path,
    current_primary_id: Option<&str>,
    is_non_primary_target: bool,
) -> Result<Vec<String>> {
    let mut workspace_ids = Vec::new();

    if !is_non_primary_target {
        if let Some(workspace_id) = current_primary_id {
            push_unique(&mut workspace_ids, workspace_id.to_string());
        }
    }

    let canonical_id =
        crate::workspace::registry::generate_workspace_id(&canonical_path.to_string_lossy())?;
    push_unique(&mut workspace_ids, canonical_id);

    Ok(workspace_ids)
}

pub(crate) fn refresh_workspace_ids_for_force_reindex(workspace_id: &str) -> Vec<String> {
    vec![workspace_id.to_string()]
}

pub(crate) async fn cancel_embedding_tasks(
    handler: &JulieServerHandler,
    workspace_ids: &[String],
    reason: &str,
) {
    let mut tasks = handler.embedding_tasks.lock().await;
    for workspace_id in workspace_ids {
        if let Some((cancel_flag, handle)) = tasks.remove(workspace_id) {
            info!(
                workspace_id = %workspace_id,
                reason,
                "Cancelling running embedding pipeline before full reindex"
            );
            cancel_flag.store(true, Ordering::Release);
            handle.abort();
        }
    }
}

pub(crate) async fn pause_force_reindex_watchers(
    handler: &JulieServerHandler,
    workspace_ids: &[String],
    pause_local_primary: bool,
) -> ForceReindexWatcherPause {
    if let Some(pool) = &handler.watcher_pool {
        let mut paused = ForceReindexWatcherPause::default();
        for workspace_id in workspace_ids {
            pool.pause_workspace(workspace_id).await;
            push_unique(
                &mut paused.watcher_pool_workspace_ids,
                workspace_id.to_string(),
            );
        }
        return paused;
    }

    if pause_local_primary {
        handler.pause_watcher().await;
        ForceReindexWatcherPause {
            watcher_pool_workspace_ids: Vec::new(),
            local_primary_paused: true,
        }
    } else {
        ForceReindexWatcherPause::default()
    }
}

pub(crate) async fn resume_force_reindex_watchers(
    handler: &JulieServerHandler,
    paused: ForceReindexWatcherPause,
) {
    if let Some(pool) = &handler.watcher_pool {
        for workspace_id in paused.watcher_pool_workspace_ids {
            pool.resume_workspace(&workspace_id).await;
        }
    }

    if paused.local_primary_paused {
        handler.resume_watcher().await;
    }
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
