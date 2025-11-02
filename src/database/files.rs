// File operations

use super::*;
use anyhow::{anyhow, Result};
use blake3;
use rusqlite::params;
use std::path::Path;
use tracing::{debug, info, warn};

/// Get current Unix timestamp in seconds, with proper error handling
fn get_unix_timestamp() -> Result<i64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| anyhow!("System time error: {}", e))
}

impl SymbolDatabase {
    pub fn store_file_info(&self, file_info: &FileInfo) -> Result<()> {
        let now = get_unix_timestamp()?;

        self.conn.execute(
            "INSERT OR REPLACE INTO files
             (path, language, hash, size, last_modified, last_indexed, symbol_count, content)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                file_info.path,
                file_info.language,
                file_info.hash,
                file_info.size,
                file_info.last_modified,
                now, // Use calculated timestamp instead of unixepoch()
                file_info.symbol_count,
                file_info.content.as_deref().unwrap_or("") // FTS5 CRITICAL: Must include content for triggers!
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

        let now = get_unix_timestamp()?;

        let mut result: Result<()> = (|| -> Result<()> {
            // 🔥 CRITICAL FIX: Wrap ENTIRE bulk operation in outer transaction for atomicity
            // If crash happens anywhere, rollback restores ALL state (triggers, indexes, files, FTS5)
            debug!("🔐 Starting atomic transaction for entire bulk file operation");
            let mut outer_tx = self.conn.transaction()?;

            // STEP 1: Disable FTS triggers (WITHIN TRANSACTION)
            debug!("🔇 Disabling FTS triggers for bulk file insert optimization");
            outer_tx.execute("DROP TRIGGER IF EXISTS files_ai", [])?;
            outer_tx.execute("DROP TRIGGER IF EXISTS files_ad", [])?;
            outer_tx.execute("DROP TRIGGER IF EXISTS files_au", [])?;

            // STEP 2: Drop indexes (WITHIN TRANSACTION)
            debug!("🗑️ Dropping file indexes for bulk insert optimization");
            let indexes = ["idx_files_language", "idx_files_modified"];
            for index in &indexes {
                outer_tx.execute(&format!("DROP INDEX IF EXISTS {}", index), [])?;
            }

            // STEP 3: Use savepoint for file inserts (nested transaction)
            let tx = outer_tx.savepoint()?;

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

            drop(stmt);
            tx.commit()?; // Commit savepoint

            // STEP 4: Rebuild FTS5 index (WITHIN OUTER TRANSACTION - atomic!)
            debug!("🔄 Rebuilding files FTS index atomically");
            outer_tx.execute("INSERT INTO files_fts(files_fts) VALUES('delete-all')", [])?;
            outer_tx.execute("INSERT INTO files_fts(files_fts) VALUES('rebuild')", [])?;

            // STEP 5: Recreate indexes (WITHIN OUTER TRANSACTION)
            debug!("🏗️ Rebuilding file indexes after bulk insert");
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_files_language ON files(language)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_files_modified ON files(last_modified)",
                [],
            )?;

            // STEP 6: Re-enable FTS triggers (WITHIN OUTER TRANSACTION)
            debug!("🔊 Re-enabling FTS triggers");
            outer_tx.execute(
                "CREATE TRIGGER IF NOT EXISTS files_ai AFTER INSERT ON files BEGIN
                    INSERT INTO files_fts(rowid, path, content)
                    VALUES (new.rowid, new.path, new.content);
                END",
                [],
            )?;
            outer_tx.execute(
                "CREATE TRIGGER IF NOT EXISTS files_ad AFTER DELETE ON files BEGIN
                    DELETE FROM files_fts WHERE rowid = old.rowid;
                END",
                [],
            )?;
            outer_tx.execute(
                "CREATE TRIGGER IF NOT EXISTS files_au AFTER UPDATE ON files BEGIN
                    UPDATE files_fts
                    SET path = new.path, content = new.content
                    WHERE rowid = old.rowid;
                END",
                [],
            )?;

            // STEP 7: Commit ENTIRE operation atomically
            debug!("💾 Committing atomic bulk file operation");
            outer_tx.commit()?;

            // Post-transaction: Non-critical WAL checkpoint
            debug!("💾 Passive WAL checkpoint (non-blocking, post-commit)");
            match self.conn.pragma_update(None, "wal_checkpoint", "PASSIVE") {
                Ok(_) => debug!("✅ Passive WAL checkpoint completed"),
                Err(e) => debug!("⚠️ Passive WAL checkpoint skipped (non-fatal): {}", e),
            }

            Ok(())
        })();

        // 🔥 ATOMICITY WIN: No manual cleanup needed!
        // If transaction failed, SQLite rolled back EVERYTHING automatically:
        // - Triggers restored to original state
        // - Indexes restored to original state
        // - Files not inserted
        // - FTS5 unchanged
        // Manual cleanup code removed - transaction guarantees consistency!

        // Restore original synchronous setting (outside transaction)
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
                "✅ Bulk file insert complete! {} files in {:.2}ms",
                files.len(),
                duration.as_millis()
            );
        }

        result
    }

    /// Drop all file table indexes for bulk operations
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
        let now = get_unix_timestamp()?;

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
        let now = get_unix_timestamp()?;

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
        language: &Option<String>,
        file_pattern: &Option<String>,
        limit: usize,
    ) -> Result<Vec<FileSearchResult>> {
        // 🔒 CRITICAL FIX: Sanitize query to prevent FTS5 syntax errors from special characters
        // This prevents errors like "fts5: syntax error near '.'" when searching for dotted names
        let sanitized_query = Self::sanitize_fts5_query(query);
        debug!(
            "🔍 FTS5 file content query sanitization: '{}' -> '{}'",
            query, sanitized_query
        );

        // Build WHERE clause dynamically based on filters
        let mut where_clauses = vec!["f MATCH ?1".to_string()];
        let mut param_index = 2;

        if language.is_some() {
            where_clauses.push(format!("files.language = ?{}", param_index));
            param_index += 1;
        }

        // Normalize file_pattern for better UX
        // Database stores canonical absolute paths (e.g., \\?\C:\source\julie\src\tests\...)
        // User expects to use relative patterns (e.g., src/tests/**)
        // Solution: Prepend wildcard to relative patterns to match any absolute prefix
        let normalized_pattern = file_pattern.as_ref().map(|pattern| {
            if pattern.starts_with('*') || pattern.starts_with('/') || pattern.starts_with('\\') {
                // Already absolute or has wildcards - use as-is
                pattern.clone()
            } else {
                // Relative pattern - prepend wildcard to match any absolute path prefix
                // Platform-aware: Use backslashes on Windows, forward slashes on Unix
                // src/tests/** becomes *\src\tests\** on Windows, */src/tests/** on Unix
                #[cfg(windows)]
                let normalized = format!("*\\{}", pattern.replace('/', "\\"));
                #[cfg(not(windows))]
                let normalized = format!("*/{}", pattern.replace('\\', "/"));
                normalized
            }
        });

        if normalized_pattern.is_some() {
            where_clauses.push(format!("f.path GLOB ?{}", param_index));
            param_index += 1;
        }

        let where_clause = where_clauses.join(" AND ");

        let query_sql = format!(
            "SELECT
                f.path,
                COALESCE(snippet(files_fts, 1, '<mark>', '</mark>', '...', 32), '[Content unavailable - file may need re-indexing]') as snippet,
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
             WHERE {}
             ORDER BY rank DESC
             LIMIT ?{}",
            where_clause, param_index
        );

        // Helper to execute the query and collect results; returns Err on SQL/FTS error
        let exec_query = |conn: &rusqlite::Connection| -> Result<Vec<FileSearchResult>> {
            let mut stmt = conn.prepare(&query_sql)?;

            // Bind parameters dynamically based on filters and collect results
            let mut results = Vec::new();

            // Use normalized_pattern for parameter binding (handles relative path conversion)
            match (language, normalized_pattern.as_ref()) {
                (Some(lang), Some(pattern)) => {
                    let mut rows = stmt.query(params![sanitized_query, lang, pattern, limit])?;
                    while let Some(row) = rows.next()? {
                        results.push(FileSearchResult {
                            path: row.get(0)?,
                            snippet: row.get(1)?,
                            rank: row.get::<_, f64>(2)? as f32,
                        });
                    }
                }
                (Some(lang), None) => {
                    let mut rows = stmt.query(params![sanitized_query, lang, limit])?;
                    while let Some(row) = rows.next()? {
                        results.push(FileSearchResult {
                            path: row.get(0)?,
                            snippet: row.get(1)?,
                            rank: row.get::<_, f64>(2)? as f32,
                        });
                    }
                }
                (None, Some(pattern)) => {
                    let mut rows = stmt.query(params![sanitized_query, pattern, limit])?;
                    while let Some(row) = rows.next()? {
                        results.push(FileSearchResult {
                            path: row.get(0)?,
                            snippet: row.get(1)?,
                            rank: row.get::<_, f64>(2)? as f32,
                        });
                    }
                }
                (None, None) => {
                    let mut rows = stmt.query(params![sanitized_query, limit])?;
                    while let Some(row) = rows.next()? {
                        results.push(FileSearchResult {
                            path: row.get(0)?,
                            snippet: row.get(1)?,
                            rank: row.get::<_, f64>(2)? as f32,
                        });
                    }
                }
            }

            Ok(results)
        };

        // First attempt
        match exec_query(&self.conn) {
            Ok(results) => Ok(results),
            Err(e) => {
                let es = e.to_string();
                // If the FTS index is desynced (common message: missing row from content table), rebuild and retry once
                if es.contains("fts5: missing row") || es.contains("invalid fts5 file format") {
                    warn!("⚠️ FTS5 query error detected ({}). Rebuilding files_fts and retrying once...", es);
                    // Attempt rebuild and retry
                    // Ignore rebuild error; if rebuild fails, return original error
                    let _ = self.rebuild_files_fts();
                    exec_query(&self.conn)
                } else {
                    Err(e)
                }
            }
        }
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

        // FTS5 CRITICAL: Rebuild index to prevent desync with external content table
        // Without this, snippet() queries will fail with "missing row X from content table"
        // when trying to access deleted rowids
        self.rebuild_files_fts()?;

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

        // FTS5 CRITICAL: Rebuild index to prevent desync
        self.rebuild_files_fts()?;

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
pub fn create_file_info<P: AsRef<Path>>(file_path: P, language: &str, workspace_root: &Path) -> Result<FileInfo> {
    let path = file_path.as_ref();
    let metadata = std::fs::metadata(path)?;
    let hash = calculate_file_hash(path)?;

    // CASCADE: Read file content for FTS5 search
    let content = std::fs::read_to_string(path).ok(); // Binary files or read errors - skip content

    let last_modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    // CRITICAL FIX: Canonicalize path to resolve symlinks, then convert to relative
    // This ensures files table and symbols table use same relative Unix-style paths
    let canonical_path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());

    // Convert to relative Unix-style path for token efficiency and cross-platform compatibility
    let relative_path = crate::utils::paths::to_relative_unix_style(&canonical_path, workspace_root)?;

    Ok(FileInfo {
        path: relative_path, // Use relative Unix-style path
        language: language.to_string(),
        hash,
        size: metadata.len() as i64,
        last_modified,
        last_indexed: 0, // Will be set by database
        symbol_count: 0, // Will be updated after extraction
        content,         // CASCADE: File content for FTS5
    })
}
