use crate::handler::JulieServerHandler;
use anyhow::Result;
use std::path::Path;

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
    crate::tools::workspace::indexing::embeddings::cancel_and_join_embedding_tasks(
        handler,
        workspace_ids,
        reason,
    )
    .await;
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.iter().any(|existing| existing == &value) {
        values.push(value);
    }
}
