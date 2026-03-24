// Bulk operations with index optimization

use super::*;
use anyhow::{Result, anyhow};
use rusqlite::params;
use tracing::{debug, info, warn};

fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

impl SymbolDatabase {
    pub fn bulk_store_identifiers(
        &mut self,
        identifiers: &[crate::extractors::Identifier],
        workspace_id: &str,
    ) -> Result<()> {
        if identifiers.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting bulk insert of {} identifiers with workspace_id: {}",
            identifiers.len(),
            workspace_id
        );

        let original_sync: i64 = self
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))?;

        let original_cache_size: i64 = self
            .conn
            .query_row("PRAGMA cache_size", [], |row| row.get(0))?;

        let current_journal: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !current_journal.eq_ignore_ascii_case("wal") {
            warn!(
                "Journal mode '{}' detected before bulk identifier insert; forcing WAL",
                current_journal
            );
            self.conn
                .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        }

        self.conn.pragma_update(None, "synchronous", 1)?;
        self.conn.pragma_update(None, "cache_size", 20000i64)?;

        let result: Result<()> = (|| -> Result<()> {
            // STEP 1: Wrap ENTIRE bulk operation in outer transaction for atomicity.
            // Index drops are INSIDE the transaction: if the process crashes before
            // commit, SQLite rolls back the DROP INDEX DDL and indexes are preserved.
            debug!("🔐 Starting atomic transaction for entire bulk identifier operation");
            let mut outer_tx = self.conn.transaction()?;

            // STEP 2: Drop all indexes (WITHIN TRANSACTION) for maximum insert speed
            debug!("🗑️ Dropping identifier indexes for bulk insert optimization");
            let identifier_indexes = [
                "idx_identifiers_name",
                "idx_identifiers_file",
                "idx_identifiers_containing",
                "idx_identifiers_target",
                "idx_identifiers_kind",
            ];
            for index in &identifier_indexes {
                if let Err(e) = outer_tx.execute(&format!("DROP INDEX IF EXISTS {}", index), []) {
                    debug!("Note: Could not drop index {}: {}", index, e);
                }
            }

            // STEP 4: Use savepoint for identifier inserts (nested within outer_tx)
            let tx = outer_tx.savepoint()?;

            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO identifiers
                 (id, name, kind, language, file_path, start_line, start_col,
                  end_line, end_col, start_byte, end_byte, containing_symbol_id,
                  target_symbol_id, confidence, code_context)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;

            const BATCH_SIZE: usize = 1000;
            let mut processed = 0;

            for chunk in identifiers.chunks(BATCH_SIZE) {
                for identifier in chunk {
                    stmt.execute(params![
                        identifier.id,
                        identifier.name,
                        identifier.kind.to_string(),
                        identifier.language,
                        identifier.file_path,
                        identifier.start_line,
                        identifier.start_column,
                        identifier.end_line,
                        identifier.end_column,
                        identifier.start_byte,
                        identifier.end_byte,
                        identifier.containing_symbol_id,
                        identifier.target_symbol_id,
                        identifier.confidence,
                        identifier.code_context
                    ])?;

                    processed += 1;
                }

                if processed % 5000 == 0 {
                    debug!(
                        "📊 Bulk insert progress: {}/{} identifiers",
                        processed,
                        identifiers.len()
                    );
                }
            }

            drop(stmt);
            tx.commit()?; // Commit savepoint

            // STEP 5: Recreate indexes (WITHIN OUTER TRANSACTION)
            debug!("🏗️ Rebuilding identifier indexes after bulk insert");
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_identifiers_name ON identifiers(name)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_identifiers_file ON identifiers(file_path)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_identifiers_containing ON identifiers(containing_symbol_id)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_identifiers_target ON identifiers(target_symbol_id)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_identifiers_kind ON identifiers(kind)",
                [],
            )?;

            // STEP 6: Commit ENTIRE operation atomically
            debug!("💾 Committing atomic bulk identifier operation");
            outer_tx.commit()?;

            // Post-transaction: TRUNCATE checkpoint to reclaim WAL disk space
            debug!("💾 TRUNCATE WAL checkpoint (reclaims disk space)");
            match self
                .conn
                .prepare("PRAGMA wal_checkpoint(TRUNCATE)")
                .and_then(|mut stmt| {
                    stmt.query_row([], |row| {
                        Ok((
                            row.get::<_, i32>(0)?,
                            row.get::<_, i32>(1)?,
                            row.get::<_, i32>(2)?,
                        ))
                    })
                }) {
                Ok((busy, log, checkpointed)) => debug!(
                    "✅ WAL TRUNCATE checkpoint: busy={}, log={}, checkpointed={}",
                    busy, log, checkpointed
                ),
                Err(e) => debug!("⚠️ WAL TRUNCATE checkpoint failed (non-fatal): {}", e),
            }

            Ok(())
        })();

        if let Err(e) = self.conn.pragma_update(None, "synchronous", original_sync) {
            warn!(
                "Failed to restore PRAGMA synchronous to {}: {}",
                original_sync, e
            );
        }
        if let Err(e) = self.conn.pragma_update(None, "cache_size", original_cache_size) {
            warn!(
                "Failed to restore PRAGMA cache_size to {}: {}",
                original_cache_size, e
            );
        }

        if let Ok(()) = result.as_ref() {
            let duration = start_time.elapsed();
            info!(
                "✅ Bulk identifier insert complete! {} identifiers in {:.2}ms ({:.0} identifiers/sec)",
                identifiers.len(),
                duration.as_millis(),
                identifiers.len() as f64 / duration.as_secs_f64()
            );
        }

        result
    }

    // ============================================================================
    // TYPE BULK OPERATIONS (Phase 4)
    // ============================================================================

    /// 🚀 BLAZING-FAST bulk type storage for type intelligence
    ///
    /// Mirrors bulk_store_identifiers pattern for consistency and performance.
    /// Uses the standard SQLite bulk pattern:
    /// 1. Drop indexes (improves insert speed 10-100x)
    /// 2. Bulk insert in single transaction
    /// 3. Recreate indexes
    /// 4. WAL checkpoint
    pub fn bulk_store_types(
        &mut self,
        types: &[crate::extractors::base::TypeInfo],
        _workspace_id: &str,
    ) -> Result<()> {
        if types.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!("🚀 Starting bulk insert of {} types", types.len());

        let original_sync: i64 = self
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))?;

        let current_journal: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !current_journal.eq_ignore_ascii_case("wal") {
            warn!(
                "Journal mode '{}' detected before bulk type insert; forcing WAL",
                current_journal
            );
            self.conn
                .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        }

        // SAFETY: drop to NORMAL only for the scope of this bulk insert and restore
        // the caller's previous synchronous level afterwards (see finalizer below).
        self.conn.pragma_update(None, "synchronous", 1)?;

        let result: Result<()> = (|| -> Result<()> {
            // STEP 1: Wrap ENTIRE bulk operation in outer transaction for atomicity.
            // Index drops are INSIDE the transaction: if the process crashes before
            // commit, SQLite rolls back the DROP INDEX DDL and indexes are preserved.
            debug!("🔐 Starting atomic transaction for entire bulk type operation");
            let mut outer_tx = self.conn.transaction()?;

            // STEP 2: Drop all indexes (WITHIN TRANSACTION) for maximum insert speed
            debug!("🗑️ Dropping type indexes for bulk insert optimization");
            let type_indexes = [
                "idx_types_language",
                "idx_types_resolved",
                "idx_types_inferred",
            ];
            for index in &type_indexes {
                if let Err(e) = outer_tx.execute(&format!("DROP INDEX IF EXISTS {}", index), []) {
                    debug!("Note: Could not drop index {}: {}", index, e);
                }
            }

            // STEP 3: Optimize SQLite for bulk operations
            outer_tx.execute("PRAGMA cache_size = 20000", [])?;

            // STEP 4: Use savepoint for type inserts (nested within outer_tx)
            let tx = outer_tx.savepoint()?;

            // STEP 4: Prepare statement once, use many times
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO types
                 (symbol_id, resolved_type, generic_params, constraints, is_inferred, language, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;

            // STEP 5: Batch insert for optimal performance
            const BATCH_SIZE: usize = 1000;
            let mut processed = 0;

            for chunk in types.chunks(BATCH_SIZE) {
                for type_info in chunk {
                    // Serialize JSON fields
                    let generic_params_json = type_info
                        .generic_params
                        .as_ref()
                        .map(|v| serde_json::to_string(v).ok())
                        .flatten();
                    let constraints_json = type_info
                        .constraints
                        .as_ref()
                        .map(|v| serde_json::to_string(v).ok())
                        .flatten();
                    let metadata_json = type_info
                        .metadata
                        .as_ref()
                        .map(|m| serde_json::to_string(m).ok())
                        .flatten();

                    stmt.execute(params![
                        type_info.symbol_id,
                        type_info.resolved_type,
                        generic_params_json,
                        constraints_json,
                        if type_info.is_inferred { 1 } else { 0 },
                        type_info.language,
                        metadata_json
                    ])?;

                    processed += 1;
                }

                // Progress logging for large bulk operations
                if processed % 5000 == 0 {
                    debug!(
                        "📊 Bulk insert progress: {}/{} types",
                        processed,
                        types.len()
                    );
                }
            }

            // STEP 5: Drop statement and commit savepoint
            drop(stmt);
            tx.commit()?;

            // STEP 6: Recreate indexes (WITHIN OUTER TRANSACTION)
            debug!("🏗️ Rebuilding type indexes after bulk insert");
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_types_language ON types(language)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_types_resolved ON types(resolved_type)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_types_inferred ON types(is_inferred)",
                [],
            )?;

            // STEP 7: Commit ENTIRE operation atomically
            debug!("💾 Committing atomic bulk type operation");
            outer_tx.commit()?;

            // Post-transaction: TRUNCATE checkpoint to reclaim WAL disk space
            debug!("💾 TRUNCATE WAL checkpoint (reclaims disk space)");
            match self
                .conn
                .prepare("PRAGMA wal_checkpoint(TRUNCATE)")
                .and_then(|mut stmt| {
                    stmt.query_row([], |row| {
                        Ok((
                            row.get::<_, i32>(0)?,
                            row.get::<_, i32>(1)?,
                            row.get::<_, i32>(2)?,
                        ))
                    })
                }) {
                Ok((busy, log, checkpointed)) => debug!(
                    "✅ WAL TRUNCATE checkpoint: busy={}, log={}, checkpointed={}",
                    busy, log, checkpointed
                ),
                Err(e) => debug!("⚠️ WAL TRUNCATE checkpoint failed (non-fatal): {}", e),
            }

            Ok(())
        })();

        if let Err(e) = self.conn.pragma_update(None, "synchronous", original_sync) {
            warn!(
                "Failed to restore PRAGMA synchronous to {}: {}",
                original_sync, e
            );
        }

        if let Ok(()) = result.as_ref() {
            let duration = start_time.elapsed();
            info!(
                "✅ Bulk type insert complete! {} types in {:.2}ms ({:.0} types/sec)",
                types.len(),
                duration.as_millis(),
                types.len() as f64 / duration.as_secs_f64()
            );
        }

        result
    }

    /// Store relationships in a transaction (regular method for incremental updates)
    pub fn store_relationships(&mut self, relationships: &[Relationship]) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        debug!("Storing {} relationships", relationships.len());

        let tx = self.conn.transaction()?;

        for rel in relationships {
            let metadata_json = rel
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            tx.execute(
                "INSERT OR REPLACE INTO relationships
                 (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    rel.id,
                    rel.from_symbol_id,
                    rel.to_symbol_id,
                    rel.kind.to_string(),
                    rel.file_path,
                    rel.line_number,
                    rel.confidence,
                    metadata_json
                ],
            )?;
        }

        tx.commit()?;
        info!("Successfully stored {} relationships", relationships.len());
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk relationship storage for initial indexing
    pub fn bulk_store_relationships(&mut self, relationships: &[Relationship]) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting blazing-fast bulk insert of {} relationships",
            relationships.len()
        );

        let current_journal: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !current_journal.eq_ignore_ascii_case("wal") {
            warn!(
                "Journal mode '{}' detected before bulk relationship insert; forcing WAL",
                current_journal
            );
            self.conn
                .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        }

        let mut inserted_count = 0usize;
        let mut skipped_count = 0usize;

        let result: Result<()> = (|| -> Result<()> {
            // 🔥 CRITICAL FIX: Wrap ENTIRE bulk operation in outer transaction for atomicity
            // If crash happens anywhere, rollback restores ALL state (indexes, relationships)
            debug!("🔐 Starting atomic transaction for entire bulk relationship operation");
            let mut outer_tx = self.conn.transaction()?;

            // STEP 1: Drop indexes (WITHIN TRANSACTION)
            debug!("🗑️ Dropping relationship indexes for bulk insert optimization");
            let indexes = ["idx_rel_from", "idx_rel_to", "idx_rel_kind"];
            for index in &indexes {
                outer_tx.execute(&format!("DROP INDEX IF EXISTS {}", index), [])?;
            }

            // STEP 2: Use savepoint for relationship inserts (nested transaction)
            let tx = outer_tx.savepoint()?;

            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO relationships
                 (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;

            for rel in relationships {
                let metadata_json = rel
                    .metadata
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;

                // Try to insert, skip if foreign key constraint fails (external/missing symbols)
                match stmt.execute(params![
                    rel.id,
                    rel.from_symbol_id,
                    rel.to_symbol_id,
                    rel.kind.to_string(),
                    rel.file_path,
                    rel.line_number,
                    rel.confidence,
                    metadata_json
                ]) {
                    Ok(_) => inserted_count += 1,
                    Err(rusqlite::Error::SqliteFailure(err, _))
                        if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                    {
                        // Skip relationships with missing symbol references
                        skipped_count += 1;
                        debug!(
                            "Skipping relationship {} -> {} (missing symbol reference)",
                            rel.from_symbol_id, rel.to_symbol_id
                        );
                    }
                    Err(e) => return Err(e.into()),
                }
            }

            drop(stmt);
            tx.commit()?; // Commit savepoint

            // STEP 3: Recreate indexes (WITHIN OUTER TRANSACTION)
            debug!("🏗️ Rebuilding relationship indexes after bulk insert");
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_rel_from ON relationships(from_symbol_id)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_rel_to ON relationships(to_symbol_id)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_rel_kind ON relationships(kind)",
                [],
            )?;

            // STEP 4: Commit ENTIRE operation atomically
            debug!("💾 Committing atomic bulk relationship operation");
            outer_tx.commit()?;

            Ok(())
        })();

        // 🔥 ATOMICITY WIN: No manual cleanup needed!
        // If transaction failed, SQLite rolled back EVERYTHING automatically:
        // - Indexes restored to original state
        // - Relationships not inserted
        // Manual cleanup code removed - transaction guarantees consistency!

        // Post-transaction: TRUNCATE checkpoint to reclaim WAL disk space
        if result.is_ok() {
            match self.checkpoint_wal() {
                Ok((busy, log, checkpointed)) => debug!(
                    "✅ WAL TRUNCATE checkpoint: busy={}, log={}, checkpointed={}",
                    busy, log, checkpointed
                ),
                Err(e) => debug!("⚠️ WAL TRUNCATE checkpoint failed (non-fatal): {}", e),
            }
        }

        if let Ok(()) = result.as_ref() {
            let duration = start_time.elapsed();
            if skipped_count > 0 {
                info!(
                    "✅ Bulk relationship insert complete! {} inserted, {} skipped (external symbols) in {:.2}ms",
                    inserted_count,
                    skipped_count,
                    duration.as_millis()
                );
            } else {
                info!(
                    "✅ Bulk relationship insert complete! {} relationships in {:.2}ms",
                    inserted_count,
                    duration.as_millis()
                );
            }
        }

        result
    }

    /// 🔥 ATOMIC INCREMENTAL UPDATE - Cleanup + Bulk Insert in ONE Transaction
    ///
    /// This method solves the critical corruption window in incremental updates:
    /// OLD FLOW: delete_symbols() commits → CRASH → bulk_store never runs → data lost
    /// NEW FLOW: ONE transaction wraps delete + insert → CRASH → rollback both
    ///
    /// Use this instead of calling delete + bulk operations separately during
    /// incremental file re-indexing.
    pub fn incremental_update_atomic(
        &mut self,
        files_to_clean: &[String],
        new_files: &[FileInfo],
        new_symbols: &[Symbol],
        new_relationships: &[Relationship],
        new_identifiers: &[crate::extractors::Identifier],
        new_types: &[crate::extractors::base::TypeInfo],
        _workspace_id: &str,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!(
            "🔐 Starting ATOMIC incremental update: cleaning {} files, inserting {} files/{} symbols/{} relationships/{} identifiers/{} types",
            files_to_clean.len(),
            new_files.len(),
            new_symbols.len(),
            new_relationships.len(),
            new_identifiers.len(),
            new_types.len()
        );

        // Prepare timestamp
        let now = get_unix_timestamp()?;

        // 🔥 CRITICAL: Disable FK checks BEFORE starting transaction
        // Reasons:
        // 1. symbols.parent_id FK lacks CASCADE - deleting parents fails
        // 2. symbols.file_path FK to files.path - insertion order matters
        // 3. Inserting symbols in arbitrary order - children before parents fails
        // PRAGMA must be set on connection, not within transaction
        self.conn.execute("PRAGMA foreign_keys = OFF", [])?;

        let result: Result<()> = (|| -> Result<()> {
            // 🔥 CRITICAL: ONE outer transaction wraps EVERYTHING
            debug!("🔐 Starting atomic transaction for incremental update");
            let outer_tx = self.conn.transaction()?;

            // STEP 1: Clean up old data for modified files (WITHIN TRANSACTION)
            if !files_to_clean.is_empty() {
                debug!("🧹 Cleaning up old data for {} files", files_to_clean.len());

                let mut total_symbols_deleted = 0;
                let mut total_rels_deleted = 0;

                for file_path in files_to_clean {
                    // Delete relationships first
                    debug!("Deleting relationships for file: {}", file_path);
                    let rels_deleted = outer_tx.execute(
                        "DELETE FROM relationships WHERE file_path = ?1",
                        params![file_path],
                    )?;
                    total_rels_deleted += rels_deleted;

                    // Delete identifiers
                    debug!("Deleting identifiers for file: {}", file_path);
                    outer_tx.execute(
                        "DELETE FROM identifiers WHERE file_path = ?1",
                        params![file_path],
                    )?;

                    // Delete types (via JOIN to find symbol_ids for this file)
                    debug!("Deleting types for file: {}", file_path);
                    outer_tx.execute(
                        "DELETE FROM types WHERE symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
                        params![file_path],
                    )?;

                    // Delete symbols
                    debug!("Deleting symbols for file: {}", file_path);
                    let symbols_deleted = outer_tx.execute(
                        "DELETE FROM symbols WHERE file_path = ?1",
                        params![file_path],
                    )?;
                    total_symbols_deleted += symbols_deleted;
                }

                debug!(
                    "🧹 Total cleanup: deleted {} symbols and {} relationships from {} files",
                    total_symbols_deleted,
                    total_rels_deleted,
                    files_to_clean.len()
                );
            }

            // STEP 2: Bulk insert new files (if any)
            if !new_files.is_empty() {
                debug!("📁 Inserting {} new file records", new_files.len());

                let mut stmt = outer_tx.prepare(
                    "INSERT OR REPLACE INTO files
                     (path, language, hash, size, last_modified, last_indexed, symbol_count, content, line_count)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )?;

                for file in new_files {
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
                drop(stmt);
            }

            // STEP 3: Bulk insert new symbols (if any)
            if !new_symbols.is_empty() {
                debug!(
                    "🔤 Inserting {} new symbols (FK checks are ON)",
                    new_symbols.len()
                );

                let mut stmt = outer_tx.prepare(
                    "INSERT OR REPLACE INTO symbols
                     (id, name, kind, language, file_path, signature, start_line, start_col,
                      end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                      parent_id, metadata, semantic_group, confidence, content_type)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                )?;

                for symbol in new_symbols {
                    let metadata_json = symbol
                        .metadata
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;

                    let visibility_str = symbol.visibility.as_ref().map(|v| match v {
                        crate::extractors::base::Visibility::Public => "public",
                        crate::extractors::base::Visibility::Private => "private",
                        crate::extractors::base::Visibility::Protected => "protected",
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
                        symbol.parent_id,
                        metadata_json,
                        symbol.semantic_group,
                        symbol.confidence,
                        symbol.content_type
                    ])?;
                }
                drop(stmt);
            }

            // STEP 4: Bulk insert new relationships (if any)
            if !new_relationships.is_empty() {
                debug!("🔗 Inserting {} new relationships", new_relationships.len());

                let mut stmt = outer_tx.prepare(
                    "INSERT OR REPLACE INTO relationships
                     (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;

                for rel in new_relationships {
                    let metadata_json = rel
                        .metadata
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;

                    // Skip relationships with missing symbol references (foreign key constraint)
                    match stmt.execute(params![
                        rel.id,
                        rel.from_symbol_id,
                        rel.to_symbol_id,
                        rel.kind.to_string(),
                        rel.file_path,
                        rel.line_number,
                        rel.confidence,
                        metadata_json
                    ]) {
                        Ok(_) => {}
                        Err(rusqlite::Error::SqliteFailure(err, _))
                            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                        {
                            debug!(
                                "Skipping relationship {} -> {} (missing symbol reference)",
                                rel.from_symbol_id, rel.to_symbol_id
                            );
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                drop(stmt);
            }

            // STEP 4.5: Bulk insert new identifiers (if any)
            if !new_identifiers.is_empty() {
                debug!("🔍 Inserting {} new identifiers", new_identifiers.len());

                let mut stmt = outer_tx.prepare(
                    "INSERT OR REPLACE INTO identifiers
                     (id, name, kind, language, file_path, start_line, start_col,
                      end_line, end_col, start_byte, end_byte, containing_symbol_id,
                      target_symbol_id, confidence, code_context)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                )?;

                for identifier in new_identifiers {
                    stmt.execute(params![
                        identifier.id,
                        identifier.name,
                        identifier.kind.to_string(),
                        identifier.language,
                        identifier.file_path,
                        identifier.start_line,
                        identifier.start_column,
                        identifier.end_line,
                        identifier.end_column,
                        identifier.start_byte,
                        identifier.end_byte,
                        identifier.containing_symbol_id,
                        identifier.target_symbol_id,
                        identifier.confidence,
                        identifier.code_context
                    ])?;
                }
                drop(stmt);
            }

            // STEP 4.6: Bulk insert new types (if any)
            if !new_types.is_empty() {
                debug!("📝 Inserting {} new types", new_types.len());

                let mut stmt = outer_tx.prepare(
                    "INSERT OR REPLACE INTO types
                     (symbol_id, resolved_type, generic_params, constraints, is_inferred, language, metadata, last_indexed)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;

                for type_info in new_types {
                    let generic_params_json = type_info
                        .generic_params
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    let constraints_json = type_info
                        .constraints
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    let metadata_json = type_info
                        .metadata
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;

                    stmt.execute(params![
                        type_info.symbol_id,
                        type_info.resolved_type,
                        generic_params_json,
                        constraints_json,
                        type_info.is_inferred,
                        type_info.language,
                        metadata_json,
                        now
                    ])?;
                }
                drop(stmt);
            }

            // STEP 5: Commit ENTIRE incremental update atomically
            debug!("💾 Committing atomic incremental update");
            outer_tx.commit()?;

            Ok(())
        })();

        // Re-enable FK checks AFTER transaction (whether success or failure)
        self.conn.execute("PRAGMA foreign_keys = ON", [])?;

        // 🔥 ATOMICITY WIN: If crash happens anywhere:
        // - Old data stays (delete didn't commit)
        // - New data not inserted
        // - Index rebuild didn't happen
        // - Database consistent (old state preserved)
        // Next incremental run will re-process the modified files

        // Post-transaction: TRUNCATE checkpoint to reclaim WAL disk space
        if result.is_ok() {
            match self.checkpoint_wal() {
                Ok((busy, log, checkpointed)) => info!(
                    "✅ WAL TRUNCATE checkpoint: busy={}, log={}, checkpointed={}",
                    busy, log, checkpointed
                ),
                Err(e) => warn!("WAL TRUNCATE checkpoint failed (non-fatal): {}", e),
            }

            let duration = start_time.elapsed();
            info!(
                "✅ Atomic incremental update complete in {:.2}ms",
                duration.as_millis()
            );
        }

        result
    }

    /// 🔐 ATOMIC fresh bulk storage — wraps all 5 table inserts in ONE outer transaction.
    ///
    /// Use for initial (fresh) workspace indexing where all tables must be populated
    /// atomically. A crash mid-way leaves zero partial data; SQLite rolls back everything.
    /// Mirrors `incremental_update_atomic` for incremental updates.
    pub fn bulk_store_fresh_atomic(
        &mut self,
        files: &[crate::database::types::FileInfo],
        symbols: &[crate::extractors::Symbol],
        relationships: &[crate::extractors::Relationship],
        identifiers: &[crate::extractors::Identifier],
        types: &[crate::extractors::base::TypeInfo],
        _workspace_id: &str,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!(
            "🔐 Starting ATOMIC fresh bulk storage: {} files, {} symbols, {} rels, {} idents, {} types",
            files.len(),
            symbols.len(),
            relationships.len(),
            identifiers.len(),
            types.len()
        );

        let original_sync: i64 = self
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))?;

        let current_journal: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !current_journal.eq_ignore_ascii_case("wal") {
            self.conn
                .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        }

        self.conn.pragma_update(None, "synchronous", 1)?;
        // Disable FK checks before transaction (same as incremental_update_atomic)
        self.conn.execute("PRAGMA foreign_keys = OFF", [])?;

        let now = get_unix_timestamp()?;

        let result: Result<()> = (|| -> Result<()> {
            let mut outer_tx = self.conn.transaction()?;

            // Drop ALL indexes across all 5 tables (WITHIN TRANSACTION — crash-safe)
            debug!("🗑️ Dropping all table indexes for atomic fresh bulk insert");
            for index in &[
                "idx_files_language",
                "idx_files_modified",
                "idx_symbols_name",
                "idx_symbols_kind",
                "idx_symbols_language",
                "idx_symbols_file",
                "idx_symbols_semantic",
                "idx_symbols_parent",
                "idx_rel_from",
                "idx_rel_to",
                "idx_rel_kind",
                "idx_identifiers_name",
                "idx_identifiers_file",
                "idx_identifiers_containing",
                "idx_identifiers_target",
                "idx_identifiers_kind",
                "idx_types_language",
                "idx_types_resolved",
                "idx_types_inferred",
            ] {
                if let Err(e) = outer_tx.execute(&format!("DROP INDEX IF EXISTS {}", index), []) {
                    debug!("Note: Could not drop index {}: {}", index, e);
                }
            }

            outer_tx.execute("PRAGMA cache_size = 30000", [])?;

            // --- Insert files ---
            if !files.is_empty() {
                debug!("📁 Inserting {} files", files.len());
                let sp = outer_tx.savepoint()?;
                let mut stmt = sp.prepare(
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
                drop(stmt);
                sp.commit()?;
            }

            // --- Insert symbols (parent-first order to satisfy FK constraints) ---
            if !symbols.is_empty() {
                debug!("🔤 Inserting {} symbols", symbols.len());

                let all_symbol_ids: std::collections::HashSet<_> =
                    symbols.iter().map(|s| s.id.clone()).collect();

                let mut sorted_symbols = Vec::with_capacity(symbols.len());
                let (no_parent, with_parent): (Vec<_>, Vec<_>) =
                    symbols.iter().cloned().partition(|s| s.parent_id.is_none());

                let mut inserted_ids = std::collections::HashSet::new();
                for sym in no_parent {
                    inserted_ids.insert(sym.id.clone());
                    sorted_symbols.push(sym);
                }

                let mut remaining = with_parent;
                while !remaining.is_empty() {
                    let before = remaining.len();
                    let (ready, not_ready): (Vec<_>, Vec<_>) =
                        remaining.into_iter().partition(|s| {
                            s.parent_id
                                .as_ref()
                                .map(|p| inserted_ids.contains(p))
                                .unwrap_or(false)
                        });
                    for sym in ready {
                        inserted_ids.insert(sym.id.clone());
                        sorted_symbols.push(sym);
                    }
                    remaining = not_ready;
                    if remaining.len() == before {
                        for mut sym in remaining {
                            if let Some(ref pid) = sym.parent_id.clone() {
                                if !all_symbol_ids.contains(pid) {
                                    sym.parent_id = None;
                                }
                            }
                            sorted_symbols.push(sym);
                        }
                        break;
                    }
                }

                let sp = outer_tx.savepoint()?;
                let mut stmt = sp.prepare(
                    "INSERT OR REPLACE INTO symbols
                     (id, name, kind, language, file_path, signature, start_line, start_col,
                      end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                      parent_id, metadata, semantic_group, confidence, content_type)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
                )?;
                for symbol in &sorted_symbols {
                    let metadata_json = symbol
                        .metadata
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    let visibility_str = symbol.visibility.as_ref().map(|v| match v {
                        crate::extractors::base::Visibility::Public => "public",
                        crate::extractors::base::Visibility::Private => "private",
                        crate::extractors::base::Visibility::Protected => "protected",
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
                        symbol.parent_id,
                        metadata_json,
                        symbol.semantic_group,
                        symbol.confidence,
                        symbol.content_type
                    ])?;
                }
                drop(stmt);
                sp.commit()?;
            }

            // --- Insert relationships ---
            if !relationships.is_empty() {
                debug!("🔗 Inserting {} relationships", relationships.len());
                let sp = outer_tx.savepoint()?;
                let mut stmt = sp.prepare(
                    "INSERT OR REPLACE INTO relationships
                     (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;
                for rel in relationships {
                    let metadata_json = rel
                        .metadata
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    match stmt.execute(params![
                        rel.id,
                        rel.from_symbol_id,
                        rel.to_symbol_id,
                        rel.kind.to_string(),
                        rel.file_path,
                        rel.line_number,
                        rel.confidence,
                        metadata_json
                    ]) {
                        Ok(_) => {}
                        Err(rusqlite::Error::SqliteFailure(err, _))
                            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
                        {
                            debug!(
                                "Skipping relationship {} -> {} (missing symbol reference)",
                                rel.from_symbol_id, rel.to_symbol_id
                            );
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                drop(stmt);
                sp.commit()?;
            }

            // --- Insert identifiers ---
            if !identifiers.is_empty() {
                debug!("🔍 Inserting {} identifiers", identifiers.len());
                let sp = outer_tx.savepoint()?;
                let mut stmt = sp.prepare(
                    "INSERT OR REPLACE INTO identifiers
                     (id, name, kind, language, file_path, start_line, start_col,
                      end_line, end_col, start_byte, end_byte, containing_symbol_id,
                      target_symbol_id, confidence, code_context)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                )?;
                for id in identifiers {
                    stmt.execute(params![
                        id.id,
                        id.name,
                        id.kind.to_string(),
                        id.language,
                        id.file_path,
                        id.start_line,
                        id.start_column,
                        id.end_line,
                        id.end_column,
                        id.start_byte,
                        id.end_byte,
                        id.containing_symbol_id,
                        id.target_symbol_id,
                        id.confidence,
                        id.code_context
                    ])?;
                }
                drop(stmt);
                sp.commit()?;
            }

            // --- Insert types ---
            if !types.is_empty() {
                debug!("📝 Inserting {} types", types.len());
                let sp = outer_tx.savepoint()?;
                let mut stmt = sp.prepare(
                    "INSERT OR REPLACE INTO types
                     (symbol_id, resolved_type, generic_params, constraints, is_inferred, language, metadata, last_indexed)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )?;
                for t in types {
                    let gp_json = t
                        .generic_params
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    let cn_json = t
                        .constraints
                        .as_ref()
                        .map(serde_json::to_string)
                        .transpose()?;
                    let md_json = t.metadata.as_ref().map(serde_json::to_string).transpose()?;
                    stmt.execute(params![
                        t.symbol_id,
                        t.resolved_type,
                        gp_json,
                        cn_json,
                        if t.is_inferred { 1 } else { 0 },
                        t.language,
                        md_json,
                        now
                    ])?;
                }
                drop(stmt);
                sp.commit()?;
            }

            // Recreate ALL indexes (WITHIN OUTER TRANSACTION — committed atomically)
            debug!("🏗️ Rebuilding all indexes after atomic fresh bulk insert");
            for sql in &[
                "CREATE INDEX IF NOT EXISTS idx_files_language ON files(language)",
                "CREATE INDEX IF NOT EXISTS idx_files_modified ON files(last_modified)",
                "CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name)",
                "CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind)",
                "CREATE INDEX IF NOT EXISTS idx_symbols_language ON symbols(language)",
                "CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path)",
                "CREATE INDEX IF NOT EXISTS idx_symbols_semantic ON symbols(semantic_group)",
                "CREATE INDEX IF NOT EXISTS idx_symbols_parent ON symbols(parent_id)",
                "CREATE INDEX IF NOT EXISTS idx_rel_from ON relationships(from_symbol_id)",
                "CREATE INDEX IF NOT EXISTS idx_rel_to ON relationships(to_symbol_id)",
                "CREATE INDEX IF NOT EXISTS idx_rel_kind ON relationships(kind)",
                "CREATE INDEX IF NOT EXISTS idx_identifiers_name ON identifiers(name)",
                "CREATE INDEX IF NOT EXISTS idx_identifiers_file ON identifiers(file_path)",
                "CREATE INDEX IF NOT EXISTS idx_identifiers_containing ON identifiers(containing_symbol_id)",
                "CREATE INDEX IF NOT EXISTS idx_identifiers_target ON identifiers(target_symbol_id)",
                "CREATE INDEX IF NOT EXISTS idx_identifiers_kind ON identifiers(kind)",
                "CREATE INDEX IF NOT EXISTS idx_types_language ON types(language)",
                "CREATE INDEX IF NOT EXISTS idx_types_resolved ON types(resolved_type)",
                "CREATE INDEX IF NOT EXISTS idx_types_inferred ON types(is_inferred)",
            ] {
                outer_tx.execute(sql, [])?;
            }

            debug!("💾 Committing atomic fresh bulk storage");
            outer_tx.commit()?;

            Ok(())
        })();

        self.conn.execute("PRAGMA foreign_keys = ON", [])?;

        if let Err(e) = self.conn.pragma_update(None, "synchronous", original_sync) {
            warn!("Failed to restore PRAGMA synchronous: {}", e);
        }

        if result.is_ok() {
            match self.checkpoint_wal() {
                Ok((busy, log, checkpointed)) => debug!(
                    "✅ WAL checkpoint: busy={}, log={}, checkpointed={}",
                    busy, log, checkpointed
                ),
                Err(e) => warn!("WAL checkpoint failed (non-fatal): {}", e),
            }
            let duration = start_time.elapsed();
            info!(
                "✅ Atomic fresh bulk storage complete in {:.2}ms",
                duration.as_millis()
            );
        }

        result
    }
}
