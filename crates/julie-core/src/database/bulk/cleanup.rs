use anyhow::{Result, bail};
use rusqlite::{Transaction, params};
use tracing::warn;

use crate::database::bulk::atomic::AtomicPersistenceMetadata;
use crate::database::revision_changes::{
    RevisionChangeKind, RevisionFileChange, record_revision_file_changes_tx,
};
use crate::database::symbols::annotations::delete_annotations_for_file;
use crate::database::{FileInfo, SymbolDatabase};

const EXTRACTOR_FAILURE_REASON: &str = "extractor_failure";

pub(super) fn delete_file_rows_tx(tx: &Transaction<'_>, file_path: &str) -> Result<()> {
    tx.execute(
        "DELETE FROM symbol_vectors WHERE symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    tx.execute(
        "DELETE FROM relationships
         WHERE file_path = ?1
            OR from_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)
            OR to_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    tx.execute(
        "UPDATE identifiers SET target_symbol_id = NULL
         WHERE target_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    // Forward-safe guard: type_arguments.target_symbol_id is currently always
    // NULL (Phase 2 emits use-site rows; symbol resolution is downstream), but
    // once it is populated, a cross-file row pointing at a symbol in THIS file
    // would dangle after the symbol delete below. FK enforcement is off during
    // bulk writes (Rule 1), so the schema's ON DELETE SET NULL never fires —
    // null it out explicitly, mirroring the identifiers guard above.
    tx.execute(
        "UPDATE type_arguments SET target_symbol_id = NULL
         WHERE target_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    // Delete this file's type_arguments dependent-first (before identifiers):
    // each row carries its own file_path, so a direct delete is exact and does
    // not rely on the identifiers/parent_arg_id CASCADE (off during bulk writes).
    tx.execute(
        "DELETE FROM type_arguments WHERE file_path = ?1",
        params![file_path],
    )?;
    // Delete this file's literals dependent-first (before symbols/files): each
    // row carries its own file_path, and may also point at a containing symbol
    // in this file. FK enforcement is off during bulk writes (Rule 1), so the
    // schema's ON DELETE CASCADE never fires — delete explicitly, matching both
    // the row's own file_path and any containing symbol owned by this file.
    tx.execute(
        "DELETE FROM literals
         WHERE file_path = ?1
            OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    tx.execute(
        "DELETE FROM source_regions
         WHERE file_path = ?1
            OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    tx.execute(
        "DELETE FROM structural_facts
         WHERE file_path = ?1
            OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    tx.execute(
        "DELETE FROM complexity_metrics
         WHERE file_path = ?1
            OR symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    // Derived web edges: drop any edge originating in this file or pointing at
    // a symbol in this file. (Edges are fully recomputed by the post-index
    // rebuild pass, but clearing here keeps the table consistent even if the
    // rebuild is skipped or fails.)
    tx.execute(
        "DELETE FROM web_edges
         WHERE file_path = ?1
            OR from_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)
            OR to_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    tx.execute(
        "DELETE FROM identifiers
         WHERE file_path = ?1
            OR containing_symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    tx.execute(
        "DELETE FROM types WHERE symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    delete_annotations_for_file(tx, file_path)?;
    tx.execute(
        "DELETE FROM indexing_repairs WHERE path = ?1",
        params![file_path],
    )?;
    tx.execute(
        "DELETE FROM symbols WHERE file_path = ?1",
        params![file_path],
    )?;
    tx.execute("DELETE FROM files WHERE path = ?1", params![file_path])?;
    Ok(())
}

pub(super) fn delete_all_indexed_rows_tx(tx: &Transaction<'_>) -> Result<()> {
    for sql in [
        "DELETE FROM symbol_vectors",
        "DELETE FROM source_regions",
        "DELETE FROM structural_facts",
        "DELETE FROM complexity_metrics",
        "DELETE FROM web_edges",
        "DELETE FROM literals",
        "DELETE FROM type_arguments",
        "DELETE FROM identifiers",
        "DELETE FROM types",
        "DELETE FROM relationships",
        "DELETE FROM symbol_annotations",
        "DELETE FROM symbols",
        "DELETE FROM files",
        "DELETE FROM indexing_repairs",
    ] {
        tx.execute(sql, [])?;
    }
    Ok(())
}

pub(super) fn record_incremental_file_changes(
    tx: &Transaction<'_>,
    revision: i64,
    workspace_id: &str,
    files_to_clean: &[String],
    new_files: &[FileInfo],
    existing_hashes: &std::collections::HashMap<String, String>,
) -> Result<()> {
    let mut changes = Vec::new();
    for file in new_files {
        let (change_kind, old_hash) = match existing_hashes.get(&file.path) {
            Some(old_hash) => (RevisionChangeKind::Modified, Some(old_hash.clone())),
            None => (RevisionChangeKind::Added, None),
        };
        changes.push(RevisionFileChange {
            revision,
            workspace_id: workspace_id.to_string(),
            file_path: file.path.clone(),
            change_kind,
            old_hash,
            new_hash: Some(file.hash.clone()),
        });
    }
    for file_path in files_to_clean {
        if new_files.iter().any(|file| file.path == *file_path) {
            continue;
        }
        if let Some(old_hash) = existing_hashes.get(file_path) {
            changes.push(RevisionFileChange {
                revision,
                workspace_id: workspace_id.to_string(),
                file_path: file_path.clone(),
                change_kind: RevisionChangeKind::Deleted,
                old_hash: Some(old_hash.clone()),
                new_hash: None,
            });
        }
    }
    record_revision_file_changes_tx(tx, revision, workspace_id, &changes)
}

pub(super) fn persist_batch_metadata_tx(
    tx: &Transaction<'_>,
    successful_files: &[FileInfo],
    metadata: AtomicPersistenceMetadata<'_>,
) -> Result<()> {
    for file in successful_files {
        tx.execute(
            "DELETE FROM indexing_repairs WHERE path = ?1",
            params![file.path],
        )?;
    }
    for (path, diagnostics) in metadata.parse_diagnostics_by_file {
        let payload = if diagnostics.is_empty() {
            None
        } else {
            Some(serde_json::to_vec(diagnostics)?)
        };
        tx.execute(
            "UPDATE files SET parse_cache = ?2 WHERE path = ?1",
            params![path, payload],
        )?;
    }
    for (path, detail) in metadata.repair_entries {
        tx.execute(
            "INSERT OR REPLACE INTO indexing_repairs (path, reason, detail, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![path, EXTRACTOR_FAILURE_REASON, detail, unix_timestamp()?],
        )?;
    }
    Ok(())
}

pub(super) fn require_workspace_id(workspace_id: &str) -> Result<()> {
    if workspace_id.trim().is_empty() {
        bail!("workspace_id is required for canonical persistence");
    }
    Ok(())
}

pub(super) fn checkpoint_wal_best_effort(db: &mut SymbolDatabase) {
    if let Err(error) = db.checkpoint_wal() {
        warn!("WAL checkpoint failed (non-fatal): {}", error);
    }
}

pub(super) fn unix_timestamp() -> Result<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64)
}
