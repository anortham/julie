use crate::handler::JulieServerHandler;
use anyhow::Result;
use std::path::Path;
use std::sync::atomic::Ordering;
use tracing::info;

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

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
