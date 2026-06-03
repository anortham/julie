use std::collections::HashSet;
use std::ptr::NonNull;

use anyhow::Result;
use rusqlite::{Connection, Transaction, params};

use crate::database::revision_changes::{
    RevisionChangeKind, RevisionFileChange, record_revision_file_changes_tx,
    snapshot_file_hashes_tx,
};
use crate::database::revisions::record_canonical_revision_tx;
use crate::database::symbols::annotations::replace_annotations_batch;
use crate::database::{CanonicalRevisionKind, FileInfo, SymbolDatabase};
use julie_extractors::{Relationship, Symbol};

use super::cleanup::{
    checkpoint_wal_best_effort, delete_all_indexed_rows_tx, delete_file_rows_tx,
    persist_batch_metadata_tx, record_incremental_file_changes, require_workspace_id,
    unix_timestamp,
};
use super::identifiers::insert_identifiers_tx;
use super::literals::insert_literals_tx;
use super::relationships::insert_relationships_tx;
use super::type_arguments::insert_type_arguments_tx;
use super::types::insert_types_tx;
pub use super::write_set::{AtomicPersistenceMetadata, CanonicalWriteSet};
use super::{collect_referenced_symbol_ids, load_existing_symbol_ids_tx};

#[derive(Default)]
struct InsertCounts {
    files: i64,
    symbols: i64,
    relationships: i64,
    identifiers: i64,
    types: i64,
    type_arguments: i64,
    literals: i64,
}

impl InsertCounts {
    fn has_changes(&self, cleaned_files: usize) -> bool {
        cleaned_files > 0
            || self.files > 0
            || self.symbols > 0
            || self.relationships > 0
            || self.identifiers > 0
            || self.types > 0
            || self.type_arguments > 0
            || self.literals > 0
    }
}

impl SymbolDatabase {
    pub fn incremental_update_atomic(
        &mut self,
        files_to_clean: &[String],
        new_files: &[FileInfo],
        new_symbols: &[Symbol],
        new_relationships: &[Relationship],
        new_identifiers: &[julie_extractors::Identifier],
        new_types: &[julie_extractors::base::TypeInfo],
        workspace_id: &str,
    ) -> Result<()> {
        let write_set = CanonicalWriteSet {
            files: new_files,
            symbols: new_symbols,
            relationships: new_relationships,
            identifiers: new_identifiers,
            types: new_types,
            // Positional convenience wrapper (tests / non-extraction callers):
            // no type-argument rows. Production paths build the full write-set
            // from ExtractedBatch / the watcher, which populate this.
            type_arguments: &[],
            literals: &[],
        };
        self.incremental_update_atomic_with_metadata(
            files_to_clean,
            &write_set,
            workspace_id,
            AtomicPersistenceMetadata::default(),
        )
        .map(|_| ())
    }

    pub fn incremental_update_atomic_with_metadata(
        &mut self,
        files_to_clean: &[String],
        write_set: &CanonicalWriteSet<'_>,
        workspace_id: &str,
        metadata: AtomicPersistenceMetadata<'_>,
    ) -> Result<Option<i64>> {
        require_workspace_id(workspace_id)?;
        let now = unix_timestamp()?;
        let fk_guard = ForeignKeyGuard::disable(&self.conn)?;

        let result = (|| {
            let tx = self.conn.transaction()?;
            let existing_hashes = snapshot_file_hashes_tx(&tx, files_to_clean)?;
            for file_path in files_to_clean {
                delete_file_rows_tx(&tx, file_path)?;
            }

            let counts = insert_batch_tx(&tx, write_set, now)?;
            let revision = if counts.has_changes(files_to_clean.len()) {
                let revision = record_canonical_revision_tx(
                    &tx,
                    workspace_id,
                    CanonicalRevisionKind::Incremental,
                    files_to_clean.len() as i64,
                    counts.files,
                    counts.symbols,
                    counts.relationships,
                    counts.identifiers,
                    counts.types,
                )?;
                record_incremental_file_changes(
                    &tx,
                    revision,
                    workspace_id,
                    files_to_clean,
                    write_set.files,
                    &existing_hashes,
                )?;
                Some(revision)
            } else {
                None
            };

            persist_batch_metadata_tx(&tx, write_set.files, metadata)?;
            if revision.is_some() && metadata.mark_external_analysis_stale {
                mark_external_analysis_stale_tx(&tx, now)?;
            }
            tx.commit()?;
            Ok(revision)
        })();

        fk_guard.restore()?;
        if result.is_ok() {
            checkpoint_wal_best_effort(self);
        }
        result
    }

    pub fn bulk_store_fresh_atomic(
        &mut self,
        files: &[FileInfo],
        symbols: &[Symbol],
        relationships: &[Relationship],
        identifiers: &[julie_extractors::Identifier],
        types: &[julie_extractors::base::TypeInfo],
        workspace_id: &str,
    ) -> Result<()> {
        let write_set = CanonicalWriteSet {
            files,
            symbols,
            relationships,
            identifiers,
            types,
            type_arguments: &[],
            literals: &[],
        };
        self.bulk_store_fresh_atomic_with_metadata(
            &write_set,
            workspace_id,
            AtomicPersistenceMetadata::default(),
        )
        .map(|_| ())
    }

    pub fn bulk_store_fresh_atomic_with_metadata(
        &mut self,
        write_set: &CanonicalWriteSet<'_>,
        workspace_id: &str,
        metadata: AtomicPersistenceMetadata<'_>,
    ) -> Result<Option<i64>> {
        fresh_insert_atomic(self, write_set, workspace_id, metadata, false)
    }

    pub fn replace_workspace_data_atomic(
        &mut self,
        write_set: &CanonicalWriteSet<'_>,
        workspace_id: &str,
        metadata: AtomicPersistenceMetadata<'_>,
    ) -> Result<Option<i64>> {
        fresh_insert_atomic(self, write_set, workspace_id, metadata, true)
    }

    pub fn delete_single_file_atomic(
        &mut self,
        workspace_id: &str,
        file_path: &str,
        metadata: AtomicPersistenceMetadata<'_>,
    ) -> Result<Option<i64>> {
        require_workspace_id(workspace_id)?;
        let now = unix_timestamp()?;
        let fk_guard = ForeignKeyGuard::disable(&self.conn)?;
        let result = (|| {
            let tx = self.conn.transaction()?;
            let paths = [file_path.to_string()];
            let existing_hashes = snapshot_file_hashes_tx(&tx, &paths)?;
            if !existing_hashes.contains_key(file_path) {
                tx.commit()?;
                return Ok(None);
            }

            delete_file_rows_tx(&tx, file_path)?;
            let revision = record_canonical_revision_tx(
                &tx,
                workspace_id,
                CanonicalRevisionKind::Incremental,
                1,
                0,
                0,
                0,
                0,
                0,
            )?;
            let changes = [RevisionFileChange {
                revision,
                workspace_id: workspace_id.to_string(),
                file_path: file_path.to_string(),
                change_kind: RevisionChangeKind::Deleted,
                old_hash: existing_hashes.get(file_path).cloned(),
                new_hash: None,
            }];
            record_revision_file_changes_tx(&tx, revision, workspace_id, &changes)?;
            if metadata.mark_external_analysis_stale {
                mark_external_analysis_stale_tx(&tx, now)?;
            }
            tx.commit()?;
            Ok(Some(revision))
        })();
        fk_guard.restore()?;
        result
    }
}

fn fresh_insert_atomic(
    db: &mut SymbolDatabase,
    write_set: &CanonicalWriteSet<'_>,
    workspace_id: &str,
    metadata: AtomicPersistenceMetadata<'_>,
    replace_existing: bool,
) -> Result<Option<i64>> {
    require_workspace_id(workspace_id)?;
    let now = unix_timestamp()?;
    let fk_guard = ForeignKeyGuard::disable(&db.conn)?;
    let result = (|| {
        let tx = db.conn.transaction()?;
        if replace_existing {
            delete_all_indexed_rows_tx(&tx)?;
        }

        let counts = insert_batch_tx(&tx, write_set, now)?;
        let revision = if counts.has_changes(0) {
            let revision = record_canonical_revision_tx(
                &tx,
                workspace_id,
                CanonicalRevisionKind::Fresh,
                0,
                counts.files,
                counts.symbols,
                counts.relationships,
                counts.identifiers,
                counts.types,
            )?;
            let changes: Vec<_> = write_set
                .files
                .iter()
                .map(|file| RevisionFileChange {
                    revision,
                    workspace_id: workspace_id.to_string(),
                    file_path: file.path.clone(),
                    change_kind: RevisionChangeKind::Added,
                    old_hash: None,
                    new_hash: Some(file.hash.clone()),
                })
                .collect();
            record_revision_file_changes_tx(&tx, revision, workspace_id, &changes)?;
            Some(revision)
        } else {
            None
        };

        persist_batch_metadata_tx(&tx, write_set.files, metadata)?;
        if revision.is_some() && metadata.mark_external_analysis_stale {
            mark_external_analysis_stale_tx(&tx, now)?;
        }
        tx.commit()?;
        Ok(revision)
    })();
    fk_guard.restore()?;
    if result.is_ok() {
        checkpoint_wal_best_effort(db);
    }
    result
}

fn insert_batch_tx(
    tx: &Transaction<'_>,
    write_set: &CanonicalWriteSet<'_>,
    now: i64,
) -> Result<InsertCounts> {
    let mut counts = InsertCounts::default();
    counts.files = insert_files_tx(tx, write_set.files, now)?;
    counts.symbols = insert_symbols_tx(tx, write_set.symbols)?;
    let valid_symbol_ids = load_existing_symbol_ids_tx(
        tx,
        &collect_referenced_symbol_ids(
            write_set.relationships,
            write_set.identifiers,
            write_set.types,
            write_set.literals,
        ),
    )?;
    counts.relationships =
        insert_relationships_tx(tx, write_set.relationships, Some(&valid_symbol_ids))?;
    counts.identifiers = insert_identifiers_tx(tx, write_set.identifiers, Some(&valid_symbol_ids))?;
    counts.types = insert_types_tx(tx, write_set.types, Some(&valid_symbol_ids), now)?;
    // type_arguments after identifiers: each row's identifier_id FKs an
    // identifier row (FK checks are off in the bulk window, but the ordering
    // keeps the dependency intent clear). target_symbol_id is write-once-NULL.
    counts.type_arguments = insert_type_arguments_tx(tx, write_set.type_arguments, now)?;
    // literals after symbols: containing_symbol_id is validated against the
    // batch's known symbols (normalized to NULL when the enclosing symbol was
    // not persisted), mirroring identifiers.
    counts.literals = insert_literals_tx(tx, write_set.literals, Some(&valid_symbol_ids))?;
    Ok(counts)
}

fn insert_files_tx(tx: &Transaction<'_>, files: &[FileInfo], now: i64) -> Result<i64> {
    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO files
         (path, language, hash, size, last_modified, last_indexed, symbol_count, content, line_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for file in files {
        stmt.execute(params![
            file.path,
            file.language,
            file.hash,
            file.size,
            file.last_modified,
            now,
            file.symbol_count,
            file.content.as_deref().unwrap_or(""),
            file.line_count
        ])?;
    }
    Ok(files.len() as i64)
}

fn insert_symbols_tx(tx: &Transaction<'_>, symbols: &[Symbol]) -> Result<i64> {
    let batch_symbol_ids: HashSet<&str> = symbols.iter().map(|symbol| symbol.id.as_str()).collect();
    let parent_ids_to_check: HashSet<String> = symbols
        .iter()
        .filter_map(|symbol| symbol.parent_id.as_deref())
        .filter(|parent_id| !batch_symbol_ids.contains(*parent_id))
        .map(str::to_string)
        .collect();
    let existing_parent_ids = load_existing_symbol_ids_tx(tx, &parent_ids_to_check)?;

    let mut stmt = tx.prepare(crate::database::helpers::SYMBOL_UPSERT_SQL)?;
    for symbol in symbols {
        let metadata_json = symbol
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let visibility_str = symbol.visibility.as_ref().map(|v| v.as_storage_str());
        let parent_id = symbol.parent_id.as_deref().filter(|parent_id| {
            batch_symbol_ids.contains(*parent_id) || existing_parent_ids.contains(*parent_id)
        });
        stmt.execute(params![
            symbol.id,
            symbol.name,
            symbol.kind.to_string(),
            symbol.language,
            symbol.file_path,
            symbol.signature,
            symbol.start_line,
            symbol.start_column,
            symbol.end_line,
            symbol.end_column,
            symbol.start_byte,
            symbol.end_byte,
            symbol.doc_comment,
            visibility_str,
            symbol.code_context,
            parent_id,
            metadata_json,
            symbol.semantic_group,
            symbol.confidence,
            symbol.content_type,
            symbol.body_span.map(|span| span.start_line),
            symbol.body_span.map(|span| span.start_column),
            symbol.body_span.map(|span| span.end_line),
            symbol.body_span.map(|span| span.end_column),
            symbol.body_span.map(|span| span.start_byte),
            symbol.body_span.map(|span| span.end_byte),
            symbol.body_hash
        ])?;
    }
    drop(stmt);
    replace_annotations_batch(tx, symbols)?;
    Ok(symbols.len() as i64)
}

fn mark_external_analysis_stale_tx(tx: &Transaction<'_>, now: i64) -> Result<()> {
    tx.execute(
        "INSERT INTO external_extract_metadata (key, value, updated_at)
         VALUES ('analysis_state', 'stale', ?1)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at",
        params![now],
    )?;
    tx.execute(
        "INSERT INTO external_extract_metadata (key, value, updated_at)
         VALUES ('analyzed_revision', '', ?1)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at",
        params![now],
    )?;
    tx.execute(
        "UPDATE external_extract_metadata
         SET value = ?1, updated_at = ?1
         WHERE key = 'updated_at'",
        params![now],
    )?;
    Ok(())
}

struct ForeignKeyGuard {
    conn: NonNull<Connection>,
    active: bool,
}

impl ForeignKeyGuard {
    fn disable(conn: &Connection) -> Result<Self> {
        conn.execute("PRAGMA foreign_keys = OFF", [])?;
        Ok(Self {
            conn: NonNull::from(conn),
            active: true,
        })
    }

    fn restore(mut self) -> Result<()> {
        // SAFETY: The guard is created from a live Connection reference and is
        // dropped before the surrounding function returns.
        unsafe {
            self.conn.as_ref().execute("PRAGMA foreign_keys = ON", [])?;
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for ForeignKeyGuard {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: Best-effort panic-path restoration for the same live
            // Connection reference used to create the guard.
            let _ = unsafe { self.conn.as_ref().execute("PRAGMA foreign_keys = ON", []) };
        }
    }
}
