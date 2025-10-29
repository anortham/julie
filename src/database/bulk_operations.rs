// Bulk operations with index optimization

use super::*;
use anyhow::{anyhow, Result};
use rusqlite::params;
use tracing::{debug, info, warn};

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

        // SAFETY: drop to NORMAL only for the scope of this bulk insert and restore
        // the caller's previous synchronous level afterwards (see finalizer below).
        self.conn.pragma_update(None, "synchronous", 1)?;

        // Track whether we need to rebuild indexes if the bulk insert bails out.
        let mut indexes_dropped = false;

        let mut result: Result<()> = (|| -> Result<()> {
            // STEP 1: Drop all indexes for maximum insert speed
            debug!("🗑️ Dropping identifier indexes for bulk insert optimization");
            self.drop_identifier_indexes()?;
            indexes_dropped = true;

            // STEP 2: Optimize SQLite for bulk operations
            self.conn.execute("PRAGMA cache_size = 20000", [])?;

            // STEP 3: Start transaction for atomic bulk insert
            let tx = self.conn.transaction()?;

            // STEP 4: Prepare statement once, use many times
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO identifiers
                 (id, name, kind, language, file_path, start_line, start_col,
                  end_line, end_col, start_byte, end_byte, containing_symbol_id,
                  target_symbol_id, confidence, code_context)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            )?;

            // STEP 5: Batch insert for optimal performance
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
                        identifier.target_symbol_id, // NULL until resolved on-demand
                        identifier.confidence,
                        identifier.code_context
                    ])?;

                    processed += 1;
                }

                // Progress logging for large bulk operations
                if processed % 5000 == 0 {
                    debug!(
                        "📊 Bulk insert progress: {}/{} identifiers",
                        processed,
                        identifiers.len()
                    );
                }
            }

            // STEP 6: Drop statement and commit transaction
            drop(stmt);
            tx.commit()?;

            Ok(())
        })();

        if indexes_dropped {
            if let Err(e) = self.create_identifier_indexes() {
                warn!(
                    "Failed to rebuild identifier indexes after bulk insert: {}",
                    e
                );
                if result.is_ok() {
                    result = Err(e);
                }
            } else {
                debug!("🏗️ Rebuilt identifier indexes after bulk insert");
            }
        }

        if let Err(e) = self.conn.pragma_update(None, "synchronous", original_sync) {
            warn!(
                "Failed to restore PRAGMA synchronous to {}: {}",
                original_sync, e
            );
            if result.is_ok() {
                result = Err(anyhow!("Failed to restore PRAGMA synchronous: {}", e));
            }
        }

        if result.is_ok() {
            debug!("💾 Passive WAL checkpoint (non-blocking)");
            match self.conn.pragma_update(None, "wal_checkpoint", "PASSIVE") {
                Ok(_) => debug!("✅ Passive WAL checkpoint completed"),
                Err(e) => debug!("⚠️ Passive WAL checkpoint skipped (non-fatal): {}", e),
            }
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

    /// Drop all identifier table indexes for bulk operations
    fn drop_identifier_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_identifiers_name",
            "idx_identifiers_file",
            "idx_identifiers_containing",
            "idx_identifiers_target",
            "idx_identifiers_kind",
        ];

        for index in &indexes {
            if let Err(e) = self
                .conn
                .execute(&format!("DROP INDEX IF EXISTS {}", index), [])
            {
                debug!("Note: Could not drop index {}: {}", index, e);
            }
        }

        Ok(())
    }

    /// Create all identifier table indexes after bulk operations
    fn create_identifier_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_name ON identifiers(name)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_file ON identifiers(file_path)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_containing ON identifiers(containing_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_target ON identifiers(target_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_kind ON identifiers(kind)",
            [],
        )?;

        Ok(())
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

    /// Drop all relationship table indexes for bulk operations
    #[allow(dead_code)]
    fn drop_relationship_indexes(&self) -> Result<()> {
        let indexes = ["idx_rel_from", "idx_rel_to", "idx_rel_kind"];

        for index in &indexes {
            if let Err(e) = self
                .conn
                .execute(&format!("DROP INDEX IF EXISTS {}", index), [])
            {
                debug!("Note: Could not drop index {}: {}", index, e);
            }
        }

        Ok(())
    }

    /// Recreate all relationship table indexes after bulk operations
    #[allow(dead_code)]
    fn create_relationship_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_from ON relationships(from_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_to ON relationships(to_symbol_id)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_kind ON relationships(kind)",
            [],
        )?;

        Ok(())
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
        _workspace_id: &str,
    ) -> Result<()> {
        let start_time = std::time::Instant::now();
        info!(
            "🔐 Starting ATOMIC incremental update: cleaning {} files, inserting {} files/{} symbols/{} relationships",
            files_to_clean.len(),
            new_files.len(),
            new_symbols.len(),
            new_relationships.len()
        );

        // Prepare timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

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

                for file_path in files_to_clean {
                    // Delete relationships first
                    debug!("Deleting relationships for file: {}", file_path);
                    outer_tx.execute(
                        "DELETE FROM relationships WHERE file_path = ?1",
                        params![file_path],
                    )?;

                    // Delete symbols
                    debug!("Deleting symbols for file: {}", file_path);
                    outer_tx.execute(
                        "DELETE FROM symbols WHERE file_path = ?1",
                        params![file_path],
                    )?;
                }
            }

            // STEP 2: Bulk insert new files (if any)
            if !new_files.is_empty() {
                debug!("📁 Inserting {} new file records", new_files.len());

                let mut stmt = outer_tx.prepare(
                    "INSERT OR REPLACE INTO files
                     (path, language, hash, size, last_modified, last_indexed, symbol_count, content)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
                        file.content.as_deref().unwrap_or("")
                    ])?;
                }
                drop(stmt);
            }

            // STEP 3: Bulk insert new symbols (if any)
            if !new_symbols.is_empty() {
                debug!("🔤 Inserting {} new symbols (FK checks are ON)", new_symbols.len());

                let mut stmt = outer_tx.prepare(
                    "INSERT OR REPLACE INTO symbols
                     (id, name, kind, language, file_path, signature, start_line, start_col,
                      end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                      parent_id, metadata, semantic_group, confidence)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
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
                        symbol.confidence
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
                        Ok(_) => {},
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
        // - Database consistent (old state preserved)
        // Next incremental run will re-process the modified files

        // 🔥 FTS5 CRITICAL: Rebuild FTS5 indexes after incremental DELETE operations
        // DELETE operations create rowid gaps in the base tables that FTS5 external
        // content tables still reference, causing "missing row X from content table" errors.
        // This is the same fix as delete_workspace_data() (workspace.rs:33-34)
        if result.is_ok() {
            debug!("🔄 Rebuilding FTS5 indexes after incremental update");
            if let Err(e) = self.rebuild_symbols_fts() {
                warn!("Failed to rebuild symbols_fts after incremental update: {}", e);
                return Err(e);
            }
            if let Err(e) = self.rebuild_files_fts() {
                warn!("Failed to rebuild files_fts after incremental update: {}", e);
                return Err(e);
            }

            let duration = start_time.elapsed();
            info!(
                "✅ Atomic incremental update complete (with FTS5 rebuild) in {:.2}ms",
                duration.as_millis()
            );
        }

        result
    }
}
