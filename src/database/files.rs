// File operations

use super::*;
use anyhow::{Result, anyhow};
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
                file_info.content.as_deref().unwrap_or("") // Content stored for Tantivy full-text indexing
            ],
        )?;

        debug!("Stored file info for: {}", file_info.path);
        Ok(())
    }

    /// Bulk file storage for initial indexing
    ///
    /// Uses optimized bulk insert pattern:
    /// 1. Drop regular indexes (improves insert speed)
    /// 2. Bulk insert in single transaction
    /// 3. Recreate regular indexes
    /// Content is stored in SQLite for Tantivy to index separately.
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
            // Wrap ENTIRE bulk operation in outer transaction for atomicity
            // If crash happens anywhere, rollback restores ALL state (indexes, files)
            debug!("🔐 Starting atomic transaction for entire bulk file operation");
            let mut outer_tx = self.conn.transaction()?;

            // STEP 1: Drop indexes (WITHIN TRANSACTION)
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

            // STEP 3: Recreate indexes (WITHIN OUTER TRANSACTION)
            debug!("🏗️ Rebuilding file indexes after bulk insert");
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_files_language ON files(language)",
                [],
            )?;
            outer_tx.execute(
                "CREATE INDEX IF NOT EXISTS idx_files_modified ON files(last_modified)",
                [],
            )?;

            // STEP 4: Commit ENTIRE operation atomically
            debug!("💾 Committing atomic bulk file operation");
            outer_tx.commit()?;

            // Post-transaction: TRUNCATE checkpoint to reclaim WAL disk space
            debug!("💾 TRUNCATE WAL checkpoint (reclaims disk space)");
            match self
                .conn
                .prepare("PRAGMA wal_checkpoint(TRUNCATE)")
                .and_then(|mut stmt| {
                    stmt.query_row([], |row| {
                        Ok((row.get::<_, i32>(0)?, row.get::<_, i32>(1)?, row.get::<_, i32>(2)?))
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

        // ATOMICITY WIN: No manual cleanup needed!
        // If transaction failed, SQLite rolled back EVERYTHING automatically:
        // - Indexes restored to original state
        // - Files not inserted
        // Transaction guarantees consistency.

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
    // CASCADE ARCHITECTURE: File Content Storage
    // ========================================

    /// Store file with full content (indexed by Tantivy for full-text search)
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

    /// Get all file contents with language for Tantivy backfill.
    /// Returns (path, language, content) tuples for files that have content stored.
    pub fn get_all_file_contents_with_language(&self) -> Result<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, language, content FROM files WHERE content IS NOT NULL",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
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

        // Tantivy index is updated separately during re-indexing

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

        // Tantivy index is updated separately during re-indexing

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
/// Reads and includes file content for Tantivy full-text search indexing
pub fn create_file_info<P: AsRef<Path>>(
    file_path: P,
    language: &str,
    workspace_root: &Path,
) -> Result<FileInfo> {
    let path = file_path.as_ref();
    let metadata = std::fs::metadata(path)?;
    let hash = calculate_file_hash(path)?;

    // Read file content for Tantivy search indexing
    let content = std::fs::read_to_string(path).ok(); // Binary files or read errors - skip content

    let last_modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    // CRITICAL FIX: Canonicalize path to resolve symlinks, then convert to relative
    // This ensures files table and symbols table use same relative Unix-style paths
    let canonical_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Convert to relative Unix-style path for token efficiency and cross-platform compatibility
    let relative_path =
        crate::utils::paths::to_relative_unix_style(&canonical_path, workspace_root)?;

    Ok(FileInfo {
        path: relative_path, // Use relative Unix-style path
        language: language.to_string(),
        hash,
        size: metadata.len() as i64,
        last_modified,
        last_indexed: 0, // Will be set by database
        symbol_count: 0, // Will be updated after extraction
        content,         // File content for Tantivy search indexing
    })
}
