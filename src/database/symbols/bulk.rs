// Bulk symbol storage operations with index optimization

use super::super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::{debug, info, warn};

impl SymbolDatabase {
    pub fn bulk_store_symbols(&mut self, symbols: &[Symbol], workspace_id: &str) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting blazing-fast bulk insert of {} symbols with workspace_id: {}",
            symbols.len(),
            workspace_id
        );

        // STEP 1: Drop all indexes for maximum insert speed
        debug!("🗑️ Dropping indexes for bulk insert optimization");
        self.drop_symbol_indexes()?;

        // STEP 2: Optimize SQLite for bulk operations (DANGEROUS but FAST!)
        self.conn.execute("PRAGMA synchronous = OFF", [])?; // No disk sync - risky but fast
                                                            // NOTE: Don't change journal_mode here - database is already in WAL mode
                                                            // Changing from WAL to MEMORY requires exclusive access and causes "database is locked" errors
                                                            // self.conn.execute_batch("PRAGMA journal_mode = MEMORY")?; // REMOVED: Causes lock conflicts
        self.conn.execute("PRAGMA cache_size = 20000", [])?; // Large cache for bulk ops

        // STEP 3: Start transaction for atomic bulk insert
        // Use regular transaction (not unchecked) to ensure foreign key constraints are enforced
        let tx = self.conn.transaction()?;

        // STEP 3.5: Insert file records first to satisfy foreign key constraints
        // Extract unique file paths with their languages from symbols
        let mut unique_files: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for symbol in symbols {
            unique_files
                .entry(symbol.file_path.clone())
                .or_insert_with(|| symbol.language.clone());
        }

        debug!("📁 Inserting {} unique file records", unique_files.len());
        let mut file_stmt = tx.prepare(
            "INSERT OR IGNORE INTO files (path, language, hash, size, last_modified, last_indexed)
             VALUES (?1, ?2, '', 0, 0, ?3)"
        )?;

        let timestamp = chrono::Utc::now().timestamp();
        for (file_path, language) in unique_files {
            file_stmt.execute(rusqlite::params![
                file_path,
                language,
                timestamp
            ])?;
        }
        drop(file_stmt);

        // STEP 4: Prepare statement once, use many times
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO symbols
             (id, name, kind, language, file_path, signature, start_line, start_col,
              end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
              parent_id, metadata, semantic_group, confidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        )?;

        // STEP 5: Sort symbols in parent-first order to avoid foreign key violations
        // Symbols with no parent go first, then their children, etc.
        let all_symbol_ids: std::collections::HashSet<_> =
            symbols.iter().map(|s| s.id.clone()).collect();

        let mut sorted_symbols = Vec::new();
        let mut remaining_symbols: Vec<_> = symbols.to_vec();
        let mut inserted_ids = std::collections::HashSet::new();

        // First pass: Insert all symbols with no parent
        let (no_parent, with_parent): (Vec<_>, Vec<_>) = remaining_symbols
            .into_iter()
            .partition(|s| s.parent_id.is_none());

        for symbol in no_parent {
            inserted_ids.insert(symbol.id.clone());
            sorted_symbols.push(symbol);
        }

        remaining_symbols = with_parent;

        // Subsequent passes: Insert symbols whose parents have been inserted
        while !remaining_symbols.is_empty() {
            let initial_count = remaining_symbols.len();
            let (can_insert, still_waiting): (Vec<_>, Vec<_>) =
                remaining_symbols.into_iter().partition(|s| {
                    s.parent_id
                        .as_ref()
                        .map(|pid| inserted_ids.contains(pid))
                        .unwrap_or(false)
                });

            for symbol in can_insert {
                inserted_ids.insert(symbol.id.clone());
                sorted_symbols.push(symbol);
            }

            remaining_symbols = still_waiting;

            // Break if we made no progress (circular dependency or orphaned symbols)
            if remaining_symbols.len() == initial_count {
                warn!(
                    "⚠️ Skipping {} symbols with unresolvable parent references",
                    remaining_symbols.len()
                );
                for mut symbol in remaining_symbols {
                    if let Some(parent_id) = &symbol.parent_id {
                        if !all_symbol_ids.contains(parent_id) {
                            debug!(
                                "Orphan symbol {} ({}) has missing parent {} - clearing relationship",
                                symbol.name,
                                symbol.id,
                                parent_id
                            );
                            symbol.parent_id = None;
                        }
                    }
                    sorted_symbols.push(symbol);
                }
                break;
            }
        }

        // Final pass: ensure no symbol references a missing parent (enforce FK safety)
        for symbol in &mut sorted_symbols {
            if let Some(parent_id) = &symbol.parent_id {
                if !all_symbol_ids.contains(parent_id) {
                    debug!(
                        "Clearing missing parent {} for symbol {} ({}) before insert",
                        parent_id, symbol.name, symbol.id
                    );
                    symbol.parent_id = None;
                }
            }
        }

        // STEP 6: Batch insert for optimal performance
        const BATCH_SIZE: usize = 1000;
        let mut processed = 0;

        // Log the first symbol for debugging
        if let Some(first_symbol) = sorted_symbols.first() {
            info!(
                "🔍 First symbol to insert: name={}, file_path={}, parent_id={:?}, id={}",
                first_symbol.name, first_symbol.file_path, first_symbol.parent_id, first_symbol.id
            );
        }

        for chunk in sorted_symbols.chunks(BATCH_SIZE) {
            for symbol in chunk {
                let metadata_json = symbol
                    .metadata
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;

                // Serialize visibility enum to string
                let visibility_str = symbol.visibility.as_ref().map(|v| match v {
                    crate::extractors::base::Visibility::Public => "public",
                    crate::extractors::base::Visibility::Private => "private",
                    crate::extractors::base::Visibility::Protected => "protected",
                });

                match stmt.execute(params![
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
                ]) {
                    Ok(_) => {}
                    Err(e) => {
                        // Log the first few failures to understand what's wrong
                        if processed < 5 {
                            warn!("Failed to insert symbol: {} from file: {} with parent: {:?}. Error: {}",
                                  symbol.name, symbol.file_path, symbol.parent_id, e);
                        }
                        return Err(anyhow::anyhow!("Symbol insertion failed: {}", e));
                    }
                }

                processed += 1;
            }

            // Progress logging for large bulk operations
            if processed % 5000 == 0 {
                debug!(
                    "📊 Bulk insert progress: {}/{} symbols",
                    processed,
                    symbols.len()
                );
            }
        }

        // STEP 6: Drop statement and commit transaction
        drop(stmt);
        tx.commit()?;

        // STEP 7: Restore safe SQLite settings
        self.conn.execute("PRAGMA synchronous = NORMAL", [])?;
        // journal_mode returns a result, so we need to use query_row or execute_batch
        self.conn.execute_batch("PRAGMA journal_mode = WAL")?;

        // STEP 8: Rebuild all indexes (still faster than incremental with indexes!)
        debug!("🏗️ Rebuilding indexes after bulk insert");
        self.create_symbol_indexes()?;

        // STEP 9: Force WAL checkpoint to flush writes to main database
        // This prevents large WAL files (>10MB) that cause "database malformed" errors during auto-checkpoint
        debug!("💾 Checkpointing WAL to flush bulk changes to main database");
        match self.conn.pragma_update(None, "wal_checkpoint", "TRUNCATE") {
            Ok(_) => debug!("✅ WAL checkpoint completed successfully"),
            Err(e) => {
                // Don't fail the operation if checkpoint fails - WAL will auto-checkpoint eventually
                warn!("⚠️ WAL checkpoint failed (non-fatal): {}", e);
            }
        }

        let duration = start_time.elapsed();
        info!(
            "✅ Blazing-fast bulk insert complete! {} symbols in {:.2}ms ({:.0} symbols/sec)",
            symbols.len(),
            duration.as_millis(),
            symbols.len() as f64 / duration.as_secs_f64()
        );

        Ok(())
    }

    /// Drop all symbol table indexes for bulk operations
    fn drop_symbol_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_symbols_name",
            "idx_symbols_kind",
            "idx_symbols_language",
            "idx_symbols_file",
            "idx_symbols_semantic",
            "idx_symbols_parent",
        ];

        for index in &indexes {
            if let Err(e) = self
                .conn
                .execute(&format!("DROP INDEX IF EXISTS {}", index), [])
            {
                // Don't fail if index doesn't exist
                debug!("Note: Could not drop index {}: {}", index, e);
            }
        }

        Ok(())
    }

    /// Recreate all symbol table indexes after bulk operations
    fn create_symbol_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_kind ON symbols(kind)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_language ON symbols(language)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_semantic ON symbols(semantic_group)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_symbols_parent ON symbols(parent_id)",
            [],
        )?;

        Ok(())
    }
}
