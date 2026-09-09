use anyhow::{Context, Result};
use julie_core::database::SymbolDatabase;
use julie_core::indexing_state::IndexingRepairReason;
use julie_core::workspace::mutation_gate::MutationGuard;
use julie_index::search::SearchIndex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

use super::FileIndexOutcome;
use super::created_modified::handle_file_created_or_modified_static;
use super::persist_repair_state;

/// Handle file deletion.
pub async fn handle_file_deleted_static(
    path: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    workspace_root: &Path,
    search_index: Option<&Arc<SearchIndex>>,
    _guard: &MutationGuard<'_>,
) -> Result<()> {
    info!("Processing file deletion: {}", path.display());

    let relative_path = julie_core::paths::to_relative_unix_style(&path, workspace_root)
        .context("Failed to convert path to relative")?;
    let workspace_key = workspace_root.to_string_lossy();
    let workspace_id = crate::workspace::registry::generate_workspace_id(&workspace_key)
        .unwrap_or_else(|_| workspace_key.into_owned());

    {
        let mut db_lock = match db.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                warn!(
                    "Database mutex poisoned during file deletion, recovering: {}",
                    poisoned
                );
                poisoned.into_inner()
            }
        };

        db_lock.delete_single_file_atomic(
            &workspace_id,
            &relative_path,
            julie_core::database::bulk::atomic::AtomicPersistenceMetadata::default(),
        )?;
        db_lock.clear_indexing_repair(&relative_path)?;

        julie_pipeline::indexing_core::web_edges::rebuild_web_edges_for_workspace(
            &mut *db_lock,
            &workspace_id,
        )?;
    }

    info!("Successfully removed indexes for {}", path.display());

    if let Some(search_index) = search_index {
        let search_index = Arc::clone(search_index);
        let rel_path = relative_path.clone();
        let tantivy_result = tokio::task::spawn_blocking(move || {
            if let Err(e) = search_index.remove_by_file_path(&rel_path) {
                warn!("Failed to remove Tantivy docs for {}: {}", rel_path, e);
            }
        })
        .await;
        if let Err(e) = tantivy_result {
            warn!("Tantivy deletion task panicked: {}", e);
        }
    }

    Ok(())
}

/// Handle file rename.
pub(crate) async fn handle_file_renamed_static(
    from: PathBuf,
    to: PathBuf,
    db: &Arc<std::sync::Mutex<SymbolDatabase>>,
    workspace_root: &Path,
    search_index: Option<&Arc<SearchIndex>>,
    guard: &MutationGuard<'_>,
) -> Result<FileIndexOutcome> {
    info!(
        "Handling file rename: {} -> {}",
        from.display(),
        to.display()
    );

    let outcome =
        handle_file_created_or_modified_static(to, db, workspace_root, search_index, guard).await?;

    if outcome.repair_reason == Some(IndexingRepairReason::ExtractorFailure) {
        return Ok(outcome);
    }

    let relative_from = julie_core::paths::to_relative_unix_style(&from, workspace_root)
        .unwrap_or_else(|_| from.to_string_lossy().replace('\\', "/"));
    if let Err(err) =
        handle_file_deleted_static(from, db, workspace_root, search_index, guard).await
    {
        persist_repair_state(
            db,
            &relative_from,
            IndexingRepairReason::DeletedFiles,
            Some(&format!("source retirement after rename failed: {err}")),
        );
        return Err(err);
    }

    Ok(outcome)
}
