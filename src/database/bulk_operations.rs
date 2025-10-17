// Bulk operations with index optimization

use super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::{debug, info};

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

        // STEP 1: Drop all indexes for maximum insert speed
        debug!("🗑️ Dropping identifier indexes for bulk insert optimization");
        self.drop_identifier_indexes()?;

        // STEP 2: Optimize SQLite for bulk operations
        self.conn.execute("PRAGMA synchronous = OFF", [])?;
        self.conn.execute_batch("PRAGMA journal_mode = MEMORY")?;
        self.conn.execute("PRAGMA cache_size = 20000", [])?;

        // STEP 3: Start transaction for atomic bulk insert
        let tx = self.conn.transaction()?;

        // STEP 4: Prepare statement once, use many times
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO identifiers
             (id, name, kind, language, file_path, start_line, start_col,
              end_line, end_col, start_byte, end_byte, containing_symbol_id,
              target_symbol_id, confidence, code_context, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
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
                    identifier.code_context,
                    workspace_id
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

        // STEP 7: Restore safe SQLite settings
        self.conn.execute("PRAGMA synchronous = NORMAL", [])?;
        self.conn.execute_batch("PRAGMA journal_mode = WAL")?;

        // STEP 8: Rebuild all indexes
        debug!("🏗️ Rebuilding identifier indexes after bulk insert");
        self.create_identifier_indexes()?;

        let duration = start_time.elapsed();
        info!(
            "✅ Bulk identifier insert complete! {} identifiers in {:.2}ms ({:.0} identifiers/sec)",
            identifiers.len(),
            duration.as_millis(),
            identifiers.len() as f64 / duration.as_secs_f64()
        );

        Ok(())
    }

    /// Drop all identifier table indexes for bulk operations
    fn drop_identifier_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_identifiers_name",
            "idx_identifiers_file",
            "idx_identifiers_containing",
            "idx_identifiers_target",
            "idx_identifiers_kind",
            "idx_identifiers_workspace",
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
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_identifiers_workspace ON identifiers(workspace_id)",
            [],
        )?;

        Ok(())
    }

    /// Store relationships in a transaction (regular method for incremental updates)
    pub fn store_relationships(
        &self,
        relationships: &[Relationship],
        workspace_id: &str,
    ) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        debug!("Storing {} relationships", relationships.len());

        let tx = self.conn.unchecked_transaction()?;

        for rel in relationships {
            let metadata_json = rel
                .metadata
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            tx.execute(
                "INSERT OR REPLACE INTO relationships
                 (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata, workspace_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    rel.id,
                    rel.from_symbol_id,
                    rel.to_symbol_id,
                    rel.kind.to_string(),
                    rel.file_path,
                    rel.line_number,
                    rel.confidence,
                    metadata_json,
                    workspace_id
                ],
            )?;
        }

        tx.commit()?;
        info!("Successfully stored {} relationships", relationships.len());
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk relationship storage for initial indexing
    pub fn bulk_store_relationships(
        &mut self,
        relationships: &[Relationship],
        workspace_id: &str,
    ) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting blazing-fast bulk insert of {} relationships",
            relationships.len()
        );

        // Drop relationship indexes
        self.drop_relationship_indexes()?;

        // Use regular transaction to ensure foreign key constraints are enforced
        let tx = self.conn.transaction()?;
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO relationships
             (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata, workspace_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;

        let mut inserted_count = 0;
        let mut skipped_count = 0;

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
                metadata_json,
                workspace_id
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

        // Rebuild relationship indexes
        self.create_relationship_indexes()?;

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

        Ok(())
    }

    /// Drop all relationship table indexes for bulk operations
    fn drop_relationship_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_rel_from",
            "idx_rel_to",
            "idx_rel_kind",
            "idx_rel_workspace",
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
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_rel_workspace ON relationships(workspace_id)",
            [],
        )?;

        Ok(())
    }
}
