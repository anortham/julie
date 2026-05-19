use anyhow::{Result, bail};

use crate::database::SymbolDatabase;
use crate::database::bulk::atomic::AtomicPersistenceMetadata;
use crate::indexing_core::batch::ExtractedBatch;

pub fn persist_force_rebuild(
    db: &mut SymbolDatabase,
    workspace_id: &str,
    batch: &ExtractedBatch,
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    db.replace_workspace_data_atomic(
        &batch.all_file_infos,
        &batch.all_symbols,
        &batch.all_relationships,
        &batch.all_identifiers,
        &batch.all_types,
        workspace_id,
        external_mutation_metadata(batch),
    )
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

    db.incremental_update_atomic_with_metadata(
        &files_to_clean,
        &batch.all_file_infos,
        &batch.all_symbols,
        &batch.all_relationships,
        &batch.all_identifiers,
        &batch.all_types,
        workspace_id,
        external_mutation_metadata(batch),
    )
}

pub fn persist_single_file_replace(
    db: &mut SymbolDatabase,
    workspace_id: &str,
    batch: &ExtractedBatch,
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    db.incremental_update_atomic_with_metadata(
        &batch.files_to_clean,
        &batch.all_file_infos,
        &batch.all_symbols,
        &batch.all_relationships,
        &batch.all_identifiers,
        &batch.all_types,
        workspace_id,
        external_mutation_metadata(batch),
    )
}

pub fn persist_single_file_delete(
    db: &mut SymbolDatabase,
    workspace_id: &str,
    file_path: &str,
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    db.delete_single_file_atomic(
        workspace_id,
        file_path,
        AtomicPersistenceMetadata {
            mark_external_analysis_stale: true,
            ..AtomicPersistenceMetadata::default()
        },
    )
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
