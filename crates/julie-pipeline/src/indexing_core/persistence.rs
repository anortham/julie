use anyhow::{bail, Result};

use crate::indexing_core::batch::ExtractedBatch;
use crate::indexing_core::web_edges::rebuild_web_edges;
use julie_core::database::bulk::atomic::AtomicPersistenceMetadata;
use julie_core::database::SymbolDatabase;

pub fn persist_force_rebuild(
    db: &mut SymbolDatabase,
    workspace_id: &str,
    batch: &ExtractedBatch,
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    let revision = db.replace_workspace_data_atomic(
        &batch.canonical_write_set(),
        workspace_id,
        external_mutation_metadata(batch),
    )?;
    // Full index: always recompute derived web edges from the freshly
    // persisted fact set.
    rebuild_web_edges(db)?;
    Ok(revision)
}

pub fn persist_incremental_scan(
    db: &mut SymbolDatabase,
    workspace_id: &str,
    batch: &ExtractedBatch,
    orphaned_files: &[String],
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    let mut files_to_clean = batch.files_to_clean.clone();
    for orphaned_file in orphaned_files {
        if !files_to_clean.iter().any(|path| path == orphaned_file) {
            files_to_clean.push(orphaned_file.clone());
        }
    }

    let revision = db.incremental_update_atomic_with_metadata(
        &files_to_clean,
        &batch.canonical_write_set(),
        workspace_id,
        external_mutation_metadata(batch),
    )?;
    // Catch-up is a bulk op; recompute derived web edges (cross-file join
    // can't be done per-file inside the atomic write).
    rebuild_web_edges(db)?;
    Ok(revision)
}

pub fn persist_single_file_replace(
    db: &mut SymbolDatabase,
    workspace_id: &str,
    batch: &ExtractedBatch,
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    let revision = db.incremental_update_atomic_with_metadata(
        &batch.files_to_clean,
        &batch.canonical_write_set(),
        workspace_id,
        external_mutation_metadata(batch),
    )?;
    // Watcher hot path: always recompute derived web edges on a file replace.
    // A replace may have removed web-relevant facts (e.g. a route-handler file
    // replaced with a non-web file), and the atomic write already deleted
    // every `web_edge` touching this file's symbols — including cross-file
    // edges from OTHER files' client calls to this file's (now-gone) handlers.
    // Gating only on the NEW facts would skip the rebuild and silently drop
    // those cross-file edges (they should degrade to external-endpoint edges).
    // Always rebuilding matches the delete path and is correct; the segcount
    // bucketing in `derive_http_call_edges` keeps the cost bounded. An
    // incremental rebuild (only edges touching the changed file's symbols)
    // is a tracked follow-up.
    rebuild_web_edges(db)?;
    Ok(revision)
}

pub fn persist_single_file_delete(
    db: &mut SymbolDatabase,
    workspace_id: &str,
    file_path: &str,
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    let revision = db.delete_single_file_atomic(
        workspace_id,
        file_path,
        AtomicPersistenceMetadata {
            mark_external_analysis_stale: true,
            ..AtomicPersistenceMetadata::default()
        },
    )?;
    // A deleted file may have been a route handler that other files' client
    // calls pointed at; recompute so those calls degrade to external edges.
    rebuild_web_edges(db)?;
    Ok(revision)
}

fn external_mutation_metadata(batch: &ExtractedBatch) -> AtomicPersistenceMetadata<'_> {
    AtomicPersistenceMetadata {
        parse_diagnostics_by_file: &batch.parse_diagnostics_by_file,
        repair_entries: &batch.repair_entries,
        mark_external_analysis_stale: true,
    }
}

fn require_workspace_id(workspace_id: &str) -> Result<()> {
    if workspace_id.trim().is_empty() {
        bail!("workspace_id is required for SQLite persistence");
    }
    Ok(())
}
