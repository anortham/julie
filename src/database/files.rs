// File operations

use super::*;
use anyhow::{anyhow, Result};
use blake3;
use rusqlite::params;
use std::path::Path;
use tracing::{debug, info, warn};

impl SymbolDatabase {
    pub fn store_file_info(&self, file_info: &FileInfo) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO files
             (path, language, hash, size, last_modified, last_indexed, symbol_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                file_info.path,
                file_info.language,
                file_info.hash,
                file_info.size,
                file_info.last_modified,
                now, // Use calculated timestamp instead of unixepoch()
                file_info.symbol_count
            ],
        )?;

        debug!("Stored file info for: {}", file_info.path);
        Ok(())
    }

    /// 🚀 BLAZING-FAST bulk file storage for initial indexing
    ///
    /// Uses the standard SQLite FTS bulk pattern:
    /// 1. Disable FTS triggers (prevents row-by-row FTS updates)
    /// 2. Drop regular indexes (improves insert speed)
    /// 3. Bulk insert in single transaction
    /// 4. Rebuild FTS once atomically
    /// 5. Recreate regular indexes
    /// 6. Re-enable FTS triggers
    pub fn bulk_store_files(&mut self, files: &[FileInfo]) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        info!(
            "🚀 Starting blazing-fast bulk insert of {} files",
            files.len()
        );

        let original_sync: i64 = self
            .conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))?;

        let current_journal: String = self
            .conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
        if !current_journal.eq_ignore_ascii_case("wal") {
            warn!(
                "Journal mode '{}' detected before bulk file insert; forcing WAL",
                current_journal
            );
            self.conn
                .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        }

        // SAFETY: drop to NORMAL only for the scope of this bulk insert and restore
        // the caller's previous synchronous level afterwards (see finalizer below).
        self.conn.pragma_update(None, "synchronous", 1)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // SAFETY: disable triggers/indexes and remember state so we can always
        // restore them even if the bulk insert returns early with an error.
        self.disable_files_fts_triggers()?;
        // Flags let us attempt to rebuild indexes/re-enable triggers even if we
        // return early with an error.
        let mut triggers_disabled = true;
        let mut indexes_need_restore = false;

        let mut result: Result<()> = (|| -> Result<()> {
            self.drop_file_indexes()?;
            indexes_need_restore = true;

            let tx = self.conn.transaction()?;
            {
                let mut stmt = tx.prepare(
                    "INSERT OR REPLACE INTO files
                     (path, language, hash, size, last_modified, last_indexed, symbol_count, content)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
                        file.content.as_deref().unwrap_or("") // CASCADE: Include content
                    ])?;
                }
            }
            tx.commit()?;

            self.rebuild_files_fts()?;

            self.create_file_indexes()?;
            indexes_need_restore = false;

            self.enable_files_fts_triggers()?;
            triggers_disabled = false;

            Ok(())
        })();

        if indexes_need_restore {
            if let Err(e) = self.create_file_indexes() {
                warn!(
                    "Failed to rebuild file indexes after bulk insert error: {}",
                    e
                );
                if result.is_ok() {
                    result = Err(e);
                }
            } else {
                debug!("🏗️ Rebuilt file indexes after recoverable error");
            }
        }

        if triggers_disabled {
            if let Err(e) = self.enable_files_fts_triggers() {
                warn!(
                    "Failed to re-enable file FTS triggers after bulk insert error: {}",
                    e
                );
                if result.is_ok() {
                    result = Err(e);
                }
            } else {
                debug!("🔊 Re-enabled file FTS triggers after recoverable error");
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
                "✅ Bulk file insert complete! {} files in {:.2}ms",
                files.len(),
                duration.as_millis()
            );
        }

        result
    }

    /// Drop all file table indexes for bulk operations
    fn drop_file_indexes(&self) -> Result<()> {
        let indexes = [
            "idx_files_language",
            "idx_files_modified",
            "idx_files_workspace",
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

    /// Recreate all file table indexes after bulk operations
    fn create_file_indexes(&self) -> Result<()> {
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_language ON files(language)",
            [],
        )?;
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_modified ON files(last_modified)",
            [],
        )?;

        Ok(())
    }

    // ========================================
    // CASCADE ARCHITECTURE: File Content Storage & FTS
    // ========================================

    /// CASCADE: Store file with full content for FTS search
    #[allow(clippy::too_many_arguments)] // Legacy API, refactor later
    pub fn store_file_with_content(
        &self,
        path: &str,
        language: &str,
        hash: &str,
        size: u64,
        last_modified: u64,
        content: &str,
        _workspace_id: &str,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT OR REPLACE INTO files
             (path, language, hash, size, last_modified, last_indexed, symbol_count, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
            params![
                path,
                language,
                hash,
                size as i64,
                last_modified as i64,
                now,
                content
            ],
        )?;

        Ok(())
    }

    /// CASCADE: Get file content from database
    pub fn get_file_content(&self, path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content FROM files WHERE path = ?1")?;

        match stmt.query_row(params![path], |row| row.get::<_, Option<String>>(0)) {
            Ok(content) => Ok(content),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// CASCADE: Get all file contents for workspace
    pub fn get_all_file_contents(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, content FROM files WHERE content IS NOT NULL")?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Get recently modified files (last N days)
    pub fn get_recent_files(&self, days: u32, limit: usize) -> Result<Vec<FileInfo>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let cutoff_time = now - (days as i64 * 86400); // days * seconds_per_day

        let mut stmt = self.conn.prepare(
            "SELECT path, language, hash, size, last_modified, last_indexed, symbol_count, content
             FROM files
             WHERE last_modified >= ?1
             ORDER BY last_modified DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![cutoff_time, limit], |row| {
            Ok(FileInfo {
                path: row.get(0)?,
                language: row.get(1)?,
                hash: row.get(2)?,
                size: row.get(3)?,
                last_modified: row.get(4)?,
                last_indexed: row.get(5)?,
                symbol_count: row.get(6)?,
                content: row.get(7)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// CASCADE: Search file content using FTS5
    pub fn search_file_content_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<FileSearchResult>> {
        // 🔒 CRITICAL FIX: Sanitize query to prevent FTS5 syntax errors from special characters
        // This prevents errors like "fts5: syntax error near '.'" when searching for dotted names
        let sanitized_query = Self::sanitize_fts5_query(query);
        debug!(
            "🔍 FTS5 file content query sanitization: '{}' -> '{}'",
            query, sanitized_query
        );

        let mut stmt = self.conn.prepare(
            "SELECT
                f.path,
                snippet(files_fts, 1, '<mark>', '</mark>', '...', 32) as snippet,
                -- Custom ranking with Lucene-style boosting
                -- 🔥 FIX: Negate BM25 (returns negative scores) so multipliers work correctly
                -- Without negation: test files get -0.17, source files get -18.00 (test files rank higher!)
                -- With negation: test files get 0.17, source files get 18.00 (source files rank higher!)
                -bm25(files_fts) *

                -- BOOST: Symbol-rich files (likely source code, not tests)
                -- Each symbol adds 5% boost (capped by symbol count)
                (1.0 + COALESCE(s.symbol_count, 0) * 0.05) *

                -- BOOST: Files in src/, lib/ (production code paths)
                -- Increased to 3.0x (was 1.5x) for stronger definition prioritization
                CASE
                    WHEN f.path GLOB '*/src/*' OR f.path GLOB '*/lib/*' THEN 3.0
                    ELSE 1.0
                END *

                -- DE-BOOST: Test files (0.01x weight - 99% reduction, strongly pushed to bottom)
                -- BM25 term frequency heavily favors test files (imports, usages, instantiations)
                -- while source files only have 1-2 definition occurrences.
                -- Very strong de-boost (0.01, was 0.1) overcomes ~10-20x term frequency advantage.
                CASE
                    WHEN f.path GLOB '*test*' OR
                         f.path GLOB '*spec*' OR
                         f.path GLOB '*__tests__*' OR
                         f.path GLOB '*.test.*' OR
                         f.path GLOB '*.spec.*'
                    THEN 0.01
                    ELSE 1.0
                END *

                -- DE-BOOST: Generated/vendor code (0.1x weight - mostly filtered out)
                CASE
                    WHEN f.path GLOB '*node_modules*' OR
                         f.path GLOB '*vendor*' OR
                         f.path GLOB '*dist/*' OR
                         f.path GLOB '*build/*' OR
                         f.path GLOB '*.min.*' OR
                         f.path GLOB '*target/debug*' OR
                         f.path GLOB '*target/release*'
                    THEN 0.1
                    ELSE 1.0
                END
                as rank

             FROM files_fts f
             LEFT JOIN files ON f.path = files.path
             LEFT JOIN (
                 -- Count symbols (functions, classes, etc.) per file
                 SELECT file_path, COUNT(*) as symbol_count
                 FROM symbols
                 WHERE kind IN ('function', 'class', 'struct', 'interface', 'method', 'impl')
                 GROUP BY file_path
             ) s ON f.path = s.file_path
             WHERE files_fts MATCH ?1
             ORDER BY rank DESC
             LIMIT ?2"
        )?;

        let rows = stmt.query_map(params![sanitized_query, limit], |row| {
            Ok(FileSearchResult {
                path: row.get(0)?,
                snippet: row.get(1)?,
                rank: row.get::<_, f64>(2)? as f32,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// Get file hash for change detection
    pub fn get_file_hash(&self, file_path: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash FROM files WHERE path = ?1")?;

        let result = stmt.query_row(params![file_path], |row| row.get::<_, String>(0));

        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Get symbol count for a file (for detecting files that need FILE_CONTENT symbols)
    pub fn get_file_symbol_count(&self, file_path: &str) -> Result<i32> {
        let mut stmt = self
            .conn
            .prepare("SELECT symbol_count FROM files WHERE path = ?1")?;

        let result = stmt.query_row(params![file_path], |row| row.get::<_, i32>(0));

        match result {
            Ok(count) => Ok(count),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(0),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Update file hash for incremental change detection
    pub fn update_file_hash(&self, file_path: &str, new_hash: &str) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.conn.execute(
            "UPDATE files SET hash = ?1, last_indexed = ?2 WHERE path = ?3",
            params![new_hash, now, file_path],
        )?;

        debug!("Updated hash for file: {}", file_path);
        Ok(())
    }

    /// Delete file record and associated symbols
    pub fn delete_file_record(&self, file_path: &str) -> Result<()> {
        // Symbols will be cascade-deleted due to foreign key constraint
        let count = self
            .conn
            .execute("DELETE FROM files WHERE path = ?1", params![file_path])?;

        debug!(
            "Deleted file record for: {} ({} rows affected)",
            file_path, count
        );
        Ok(())
    }

    /// Delete file record for a specific workspace (workspace-aware cleanup)
    pub fn delete_file_record_in_workspace(&self, file_path: &str) -> Result<()> {
        let count = self
            .conn
            .execute("DELETE FROM files WHERE path = ?1", params![file_path])?;

        debug!(
            "Deleted file record for '{}' ({} rows affected)",
            file_path, count
        );
        Ok(())
    }

    /// Store symbols in a transaction (regular method for incremental updates)
    pub fn get_file_hashes_for_workspace(
        &self,
    ) -> Result<std::collections::HashMap<String, String>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, hash
             FROM files
             ORDER BY path",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?, // path
                row.get::<_, String>(1)?, // hash
            ))
        })?;

        let mut file_hashes = std::collections::HashMap::new();
        for row_result in rows {
            let (path, hash) = row_result?;
            file_hashes.insert(path, hash);
        }

        debug!("Retrieved {} file hashes from database", file_hashes.len());
        Ok(file_hashes)
    }
}

pub fn calculate_file_hash<P: AsRef<Path>>(file_path: P) -> Result<String> {
    let content = std::fs::read(file_path)?;
    let hash = blake3::hash(&content);
    Ok(hash.to_hex().to_string())
}

/// Create FileInfo from a file path
/// CASCADE: Now reads and includes file content for FTS5 search
pub fn create_file_info<P: AsRef<Path>>(file_path: P, language: &str) -> Result<FileInfo> {
    let path = file_path.as_ref();
    let metadata = std::fs::metadata(path)?;
    let hash = calculate_file_hash(path)?;

    // CASCADE: Read file content for FTS5 search
    let content = std::fs::read_to_string(path).ok(); // Binary files or read errors - skip content

    let last_modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    // CRITICAL FIX: Canonicalize path to resolve symlinks (macOS /var vs /private/var)
    // This ensures files table and symbols table use same canonical paths
    // Without this: files table has /var/..., symbols have /private/var/... → FOREIGN KEY fail
    let canonical_path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string();

    Ok(FileInfo {
        path: canonical_path, // Use canonical path, not original
        language: language.to_string(),
        hash,
        size: metadata.len() as i64,
        last_modified,
        last_indexed: 0, // Will be set by database
        symbol_count: 0, // Will be updated after extraction
        content,         // CASCADE: File content for FTS5
    })
}
