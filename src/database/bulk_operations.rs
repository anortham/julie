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

        let mut indexes_dropped = false;
        let mut inserted_count = 0usize;
        let mut skipped_count = 0usize;

        let mut result: Result<()> = (|| -> Result<()> {
            self.drop_relationship_indexes()?;
            indexes_dropped = true;

            // Use regular transaction to ensure foreign key constraints are enforced
            let tx = self.conn.transaction()?;
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

            // Drop statement before committing transaction
            drop(stmt);
            tx.commit()?;

            Ok(())
        })();

        if indexes_dropped {
            if let Err(e) = self.create_relationship_indexes() {
                warn!(
                    "Failed to rebuild relationship indexes after bulk insert: {}",
                    e
                );
                if result.is_ok() {
                    result = Err(e);
                }
            } else {
                debug!("🏗️ Rebuilt relationship indexes after bulk insert");
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

    /// Drop all relationship table indexes for bulk operations
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
}
