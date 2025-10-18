// Bulk symbol storage operations with index optimization

use super::super::*;
use anyhow::{anyhow, Result};
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

        let original_sync: i64 = self
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))?;

        let current_journal: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if current_journal.to_ascii_lowercase() != "wal" {
            warn!(
                "Journal mode '{}' detected before bulk symbol insert; forcing WAL",
                current_journal
            );
            self.conn
                .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        }

        // SAFETY: lower synchronous to NORMAL just for the duration of this bulk
        // insert to keep write performance high while guaranteeing we restore the
        // caller's original durability guarantees once we're done.
        self.conn.pragma_update(None, "synchronous", 1)?;

        // These flags ensure we rebuild indexes and re-enable FTS triggers even when
        // the bulk insert fails somewhere mid-flight.
        let mut triggers_disabled = false;
        let mut indexes_need_restore = false;
        let mut processed = 0usize;

        let mut result: Result<()> = (|| -> Result<()> {
            // STEP 1: Disable FTS triggers to prevent row-by-row FTS updates
            debug!("🔇 Disabling FTS triggers for bulk insert optimization");
            self.disable_symbols_fts_triggers()?;
            triggers_disabled = true;

            // STEP 2: Drop all indexes for maximum insert speed
            debug!("🗑️ Dropping indexes for bulk insert optimization");
            self.drop_symbol_indexes()?;
            indexes_need_restore = true;

            // STEP 3: Optimize SQLite cache for bulk operations
            self.conn.execute("PRAGMA cache_size = 20000", [])?;

            // STEP 4: Start transaction for atomic bulk insert
            let tx = self.conn.transaction()?;

            // STEP 4.5: Insert file records first to satisfy foreign key constraints
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
                 VALUES (?1, ?2, '', 0, 0, ?3)",
            )?;

            let timestamp = chrono::Utc::now().timestamp();
            for (file_path, language) in unique_files {
                file_stmt.execute(rusqlite::params![file_path, language, timestamp])?;
            }
            drop(file_stmt);

            // STEP 5: Prepare statement once, use many times
            let mut stmt = tx.prepare(
                "INSERT OR REPLACE INTO symbols
                 (id, name, kind, language, file_path, signature, start_line, start_col,
                  end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                  parent_id, metadata, semantic_group, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            )?;

            // STEP 5.5: Sort symbols in parent-first order to avoid foreign key violations
            let all_symbol_ids: std::collections::HashSet<_> =
                symbols.iter().map(|s| s.id.clone()).collect();

            let mut sorted_symbols = Vec::new();
            let mut remaining_symbols: Vec<_> = symbols.to_vec();
            let mut inserted_ids = std::collections::HashSet::new();

            let (no_parent, with_parent): (Vec<_>, Vec<_>) = remaining_symbols
                .into_iter()
                .partition(|s| s.parent_id.is_none());

            for symbol in no_parent {
                inserted_ids.insert(symbol.id.clone());
                sorted_symbols.push(symbol);
            }

            remaining_symbols = with_parent;

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

            const BATCH_SIZE: usize = 1000;
            processed = 0;

            if let Some(first_symbol) = sorted_symbols.first() {
                info!(
                    "🔍 First symbol to insert: name={}, file_path={}, parent_id={:?}, id={}",
                    first_symbol.name,
                    first_symbol.file_path,
                    first_symbol.parent_id,
                    first_symbol.id
                );
            }

            for chunk in sorted_symbols.chunks(BATCH_SIZE) {
                for symbol in chunk {
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
                            if processed < 5 {
                                warn!(
                                    "Failed to insert symbol: {} from file: {} with parent: {:?}. Error: {}",
                                    symbol.name, symbol.file_path, symbol.parent_id, e
                                );
                            }
                            return Err(anyhow!("Symbol insertion failed: {}", e));
                        }
                    }

                    processed += 1;
                }

                if processed % 5000 == 0 {
                    debug!(
                        "📊 Bulk insert progress: {}/{} symbols",
                        processed,
                        symbols.len()
                    );
                }
            }

            drop(stmt);
            tx.commit()?;

            debug!("🔄 Rebuilding symbols FTS index atomically");
            self.rebuild_symbols_fts()?;

            debug!("🏗️ Rebuilding symbol indexes after bulk insert");
            self.create_symbol_indexes()?;
            indexes_need_restore = false;

            debug!("🔊 Re-enabling FTS triggers");
            self.enable_symbols_fts_triggers()?;
            triggers_disabled = false;

            debug!("💾 Passive WAL checkpoint (non-blocking)");
            match self.conn.pragma_update(None, "wal_checkpoint", "PASSIVE") {
                Ok(_) => debug!("✅ Passive WAL checkpoint completed"),
                Err(e) => debug!("⚠️ Passive WAL checkpoint skipped (non-fatal): {}", e),
            }

            Ok(())
        })();

        if indexes_need_restore {
            if let Err(e) = self.create_symbol_indexes() {
                warn!("Failed to rebuild symbol indexes after error: {}", e);
                if result.is_ok() {
                    result = Err(e);
                }
            } else {
                debug!("🏗️ Rebuilt symbol indexes after recoverable error");
            }
        }

        if triggers_disabled {
            if let Err(e) = self.enable_symbols_fts_triggers() {
                warn!("Failed to re-enable symbol FTS triggers after error: {}", e);
                if result.is_ok() {
                    result = Err(e);
                }
            } else {
                debug!("🔊 Re-enabled symbol FTS triggers after recoverable error");
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

        if let Ok(()) = result.as_ref() {
            let duration = start_time.elapsed();
            info!(
                "✅ Blazing-fast bulk insert complete! {} symbols in {:.2}ms ({:.0} symbols/sec)",
                symbols.len(),
                duration.as_millis(),
                symbols.len() as f64 / duration.as_secs_f64()
            );
        }

        result
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
