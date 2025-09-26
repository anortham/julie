// Julie's Database Module - SQLite Source of Truth
//!
//! This module provides persistent storage for symbols, relationships, files, and metadata
//! using SQLite as the foundation of Julie's three-pillar architecture.
//!
//! Key features:
//! - Comprehensive schema for cross-language symbol storage
//! - File tracking with Blake3 hashing for incremental updates
//! - Rich relationship mapping for data flow tracing
//! - Efficient indexes for sub-100ms query performance

use anyhow::{anyhow, Result};
use rusqlite::{Connection, params, Row};
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use serde::{Serialize, Deserialize};

use crate::extractors::{Symbol, Relationship, SymbolKind, RelationshipKind};

/// The main database connection and operations
pub struct SymbolDatabase {
    conn: Connection,
    file_path: PathBuf,
}

/// File tracking information with Blake3 hashing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub hash: String,  // Blake3 hash
    pub size: i64,
    pub last_modified: i64,  // Unix timestamp
    pub last_indexed: i64,   // Unix timestamp
    pub symbol_count: i32,
}

/// Embedding metadata linking symbols to vector store
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingInfo {
    pub symbol_id: String,
    pub vector_id: String,
    pub model_name: String,
    pub embedding_hash: String,
    pub created_at: i64,
}

/// Database statistics for health monitoring
#[derive(Debug)]
pub struct DatabaseStats {
    pub total_symbols: i64,
    pub total_relationships: i64,
    pub total_files: i64,
    pub total_embeddings: i64,
    pub languages: Vec<String>,
    pub db_size_mb: f64,
}

impl SymbolDatabase {
    /// Create a new database connection and initialize schema
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let file_path = db_path.as_ref().to_path_buf();

        info!("Initializing SQLite database at: {}", file_path.display());

        let conn = Connection::open(&file_path)
            .map_err(|e| anyhow!("Failed to open database: {}", e))?;

        let mut db = Self { conn, file_path };
        db.initialize_schema()?;

        info!("Database initialized successfully");
        Ok(db)
    }

    /// Initialize the complete database schema
    fn initialize_schema(&mut self) -> Result<()> {
        debug!("Creating database schema");

        // Enable foreign key constraints
        self.conn.execute("PRAGMA foreign_keys = ON", [])?;

        // Set WAL mode for better concurrency (this returns results, so ignore them)
        let _ = self.conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;

        // Create tables in dependency order
        self.create_files_table()?;
        self.create_symbols_table()?;
        self.create_relationships_table()?;
        self.create_embeddings_table()?;

        debug!("Database schema created successfully");
        Ok(())
    }

    /// Create the files table for tracking source files
    fn create_files_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS files (
                path TEXT PRIMARY KEY,
                language TEXT NOT NULL,
                hash TEXT NOT NULL,
                size INTEGER NOT NULL,
                last_modified INTEGER NOT NULL,
                last_indexed INTEGER DEFAULT 0,
                parse_cache BLOB,
                symbol_count INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Indexes for file queries
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_language ON files(language)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_files_modified ON files(last_modified)",
            [],
        )?;

        debug!("Created files table and indexes");
        Ok(())
    }

    /// Create the symbols table with rich metadata
    fn create_symbols_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                language TEXT NOT NULL,
                file_path TEXT NOT NULL REFERENCES files(path) ON DELETE CASCADE,
                signature TEXT,
                start_line INTEGER,
                start_col INTEGER,
                end_line INTEGER,
                end_col INTEGER,
                parent_id TEXT REFERENCES symbols(id),
                metadata TEXT,  -- JSON blob

                -- For incremental updates
                file_hash TEXT,
                last_indexed INTEGER DEFAULT 0,

                -- For cross-language linking
                semantic_group TEXT,
                confidence REAL DEFAULT 1.0
            )",
            [],
        )?;

        // Essential indexes for fast queries
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

        debug!("Created symbols table and indexes");
        Ok(())
    }

    /// Create the relationships table for tracing data flow
    fn create_relationships_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS relationships (
                id TEXT PRIMARY KEY,
                from_symbol_id TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                to_symbol_id TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                confidence REAL DEFAULT 1.0,
                metadata TEXT,  -- JSON blob
                created_at INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Indexes for relationship traversal
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

        debug!("Created relationships table and indexes");
        Ok(())
    }

    /// Create the embeddings table for vector mapping
    fn create_embeddings_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS embeddings (
                symbol_id TEXT NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                vector_id TEXT NOT NULL,
                model_name TEXT NOT NULL,
                embedding_hash TEXT,
                created_at INTEGER DEFAULT 0,

                PRIMARY KEY (symbol_id, model_name)
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_embeddings_vector ON embeddings(vector_id)",
            [],
        )?;

        debug!("Created embeddings table and indexes");
        Ok(())
    }

    /// Store file information with Blake3 hash
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

    /// Get file hash for change detection
    pub fn get_file_hash(&self, file_path: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT hash FROM files WHERE path = ?1"
        )?;

        let result = stmt.query_row(params![file_path], |row| {
            Ok(row.get::<_, String>(0)?)
        });

        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
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
        let count = self.conn.execute(
            "DELETE FROM files WHERE path = ?1",
            params![file_path],
        )?;

        debug!("Deleted file record for: {} ({} rows affected)", file_path, count);
        Ok(())
    }

    /// Store symbols in a transaction
    pub async fn store_symbols(&self, symbols: &[Symbol]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        debug!("Storing {} symbols", symbols.len());

        let tx = self.conn.unchecked_transaction()?;

        for symbol in symbols {
            let metadata_json = symbol.metadata.as_ref()
                .map(|m| serde_json::to_string(m))
                .transpose()?;

            tx.execute(
                "INSERT OR REPLACE INTO symbols
                 (id, name, kind, language, file_path, signature, start_line, start_col,
                  end_line, end_col, parent_id, metadata, semantic_group, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    symbol.id,
                    symbol.name,
                    symbol.kind.to_string(),
                    symbol.language,
                    symbol.file_path,
                    symbol.signature,
                    symbol.start_line,
                    symbol.start_column, // This matches start_col in table
                    symbol.end_line,
                    symbol.end_column,   // This matches end_col in table
                    symbol.parent_id,
                    metadata_json,
                    symbol.semantic_group,
                    symbol.confidence
                ],
            )?;
        }

        tx.commit()?;
        info!("Successfully stored {} symbols", symbols.len());
        Ok(())
    }

    /// Store relationships in a transaction
    pub async fn store_relationships(&self, relationships: &[Relationship]) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        debug!("Storing {} relationships", relationships.len());

        let tx = self.conn.unchecked_transaction()?;

        for rel in relationships {
            let metadata_json = rel.metadata.as_ref()
                .map(|m| serde_json::to_string(m))
                .transpose()?;

            tx.execute(
                "INSERT OR REPLACE INTO relationships
                 (id, from_symbol_id, to_symbol_id, kind, confidence, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    rel.id,
                    rel.from_symbol_id,
                    rel.to_symbol_id,
                    rel.kind.to_string(),
                    rel.confidence,
                    metadata_json
                ],
            )?;
        }

        tx.commit()?;
        info!("Successfully stored {} relationships", relationships.len());
        Ok(())
    }

    /// Get symbol by ID
    pub async fn get_symbol_by_id(&self, id: &str) -> Result<Option<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, parent_id, metadata, semantic_group, confidence
             FROM symbols WHERE id = ?1"
        )?;

        let result = stmt.query_row(params![id], |row| {
            self.row_to_symbol(row)
        });

        match result {
            Ok(symbol) => Ok(Some(symbol)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Find symbols by name with optional language filter
    pub async fn find_symbols_by_name(&self, name: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE name = ?1
             ORDER BY language, file_path"
        )?;

        let symbol_iter = stmt.query_map(params![name], |row| {
            self.row_to_symbol(row)
        })?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!("Found {} symbols named '{}'", symbols.len(), name);
        Ok(symbols)
    }

    /// Get symbols for a specific file
    pub async fn get_symbols_for_file(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE file_path = ?1
             ORDER BY start_line, start_col"
        )?;

        let symbol_iter = stmt.query_map(params![file_path], |row| {
            self.row_to_symbol(row)
        })?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!("Found {} symbols in file '{}'", symbols.len(), file_path);
        Ok(symbols)
    }

    /// Delete symbols for a specific file (for incremental updates)
    pub async fn delete_symbols_for_file(&self, file_path: &str) -> Result<()> {
        let count = self.conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;

        debug!("Deleted {} symbols from file '{}'", count, file_path);
        Ok(())
    }

    /// Get outgoing relationships from a symbol
    pub async fn get_outgoing_relationships(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_symbol_id, to_symbol_id, kind, confidence, metadata
             FROM relationships
             WHERE from_symbol_id = ?1"
        )?;

        let rel_iter = stmt.query_map(params![symbol_id], |row| {
            self.row_to_relationship(row)
        })?;

        let mut relationships = Vec::new();
        for rel_result in rel_iter {
            relationships.push(rel_result?);
        }

        debug!("Found {} outgoing relationships from symbol '{}'", relationships.len(), symbol_id);
        Ok(relationships)
    }

    /// Begin a database transaction
    pub fn begin_transaction(&mut self) -> Result<()> {
        self.conn.execute("BEGIN TRANSACTION", [])?;
        Ok(())
    }

    /// Commit the current transaction
    pub fn commit_transaction(&self) -> Result<()> {
        self.conn.execute("COMMIT", [])?;
        Ok(())
    }

    /// Rollback the current transaction
    pub fn rollback_transaction(&self) -> Result<()> {
        self.conn.execute("ROLLBACK", [])?;
        Ok(())
    }

    /// Get database statistics
    pub fn get_stats(&self) -> Result<DatabaseStats> {
        let total_symbols: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols",
            [],
            |row| row.get(0)
        )?;

        let total_relationships: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM relationships",
            [],
            |row| row.get(0)
        )?;

        let total_files: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files",
            [],
            |row| row.get(0)
        )?;

        let total_embeddings: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM embeddings",
            [],
            |row| row.get(0)
        )?;

        // Get unique languages
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT language FROM files ORDER BY language"
        )?;

        let language_iter = stmt.query_map([], |row| {
            Ok(row.get::<_, String>(0)?)
        })?;

        let mut languages = Vec::new();
        for lang_result in language_iter {
            languages.push(lang_result?);
        }

        // Get database file size
        let db_size_mb = if let Ok(metadata) = std::fs::metadata(&self.file_path) {
            metadata.len() as f64 / (1024.0 * 1024.0)
        } else {
            0.0
        };

        Ok(DatabaseStats {
            total_symbols,
            total_relationships,
            total_files,
            total_embeddings,
            languages,
            db_size_mb,
        })
    }

    /// Helper to convert database row to Symbol
    fn row_to_symbol(&self, row: &Row) -> rusqlite::Result<Symbol> {
        let kind_str: String = row.get("kind")?;
        let kind = SymbolKind::from_string(&kind_str);

        let metadata_json: Option<String> = row.get("metadata")?;
        let metadata = metadata_json
            .and_then(|json| serde_json::from_str(&json).ok());

        Ok(Symbol {
            id: row.get("id")?,
            name: row.get("name")?,
            kind,
            language: row.get("language")?,
            file_path: row.get("file_path")?,
            signature: row.get("signature")?,
            start_line: row.get("start_line")?,
            start_column: row.get("start_col")?,
            end_line: row.get("end_line")?,
            end_column: row.get("end_col")?,
            start_byte: 0, // TODO: Add start_byte to database
            end_byte: 0, // TODO: Add end_byte to database
            doc_comment: None, // TODO: Add doc_comment to database
            visibility: None, // TODO: Add visibility to database
            parent_id: row.get("parent_id")?,
            metadata,
            semantic_group: row.get("semantic_group")?,
            confidence: row.get("confidence")?,
            code_context: None, // TODO: Add code_context to database schema
        })
    }

    /// Helper to convert database row to Relationship
    fn row_to_relationship(&self, row: &Row) -> rusqlite::Result<Relationship> {
        let kind_str: String = row.get("kind")?;
        let kind = RelationshipKind::from_string(&kind_str);

        let metadata_json: Option<String> = row.get("metadata")?;
        let metadata = metadata_json
            .and_then(|json| serde_json::from_str(&json).ok());

        Ok(Relationship {
            id: row.get("id")?,
            from_symbol_id: row.get("from_symbol_id")?,
            to_symbol_id: row.get("to_symbol_id")?,
            kind,
            file_path: String::new(), // TODO: Add file_path to relationship storage
            line_number: 0, // TODO: Add line_number to relationship storage
            confidence: row.get("confidence").unwrap_or(1.0),
            metadata,
        })
    }

    /// Get relationships where the specified symbol is the source (from_symbol_id)
    pub async fn get_relationships_for_symbol(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare("
            SELECT id, from_symbol_id, to_symbol_id, kind, confidence, metadata
            FROM relationships
            WHERE from_symbol_id = ?1
        ")?;

        let rows = stmt.query_map([symbol_id], |row| {
            self.row_to_relationship(row)
        })?;

        let mut relationships = Vec::new();
        for row_result in rows {
            relationships.push(row_result?);
        }

        Ok(relationships)
    }

    /// Get symbols grouped by semantic_group field
    pub async fn get_symbols_by_semantic_group(&self, semantic_group: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare("
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE semantic_group = ?1
        ")?;

        let rows = stmt.query_map([semantic_group], |row| {
            self.row_to_symbol(row)
        })?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        Ok(symbols)
    }
}

/// Utility function to calculate Blake3 hash of file content
pub fn calculate_file_hash<P: AsRef<Path>>(file_path: P) -> Result<String> {
    let content = std::fs::read(file_path)?;
    let hash = blake3::hash(&content);
    Ok(hash.to_hex().to_string())
}

/// Create FileInfo from a file path
pub fn create_file_info<P: AsRef<Path>>(
    file_path: P,
    language: &str,
) -> Result<FileInfo> {
    let path = file_path.as_ref();
    let metadata = std::fs::metadata(path)?;
    let hash = calculate_file_hash(path)?;

    let last_modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;

    Ok(FileInfo {
        path: path.to_string_lossy().to_string(),
        language: language.to_string(),
        hash,
        size: metadata.len() as i64,
        last_modified,
        last_indexed: 0, // Will be set by database
        symbol_count: 0, // Will be updated after extraction
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::extractors::SymbolKind;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let db = SymbolDatabase::new(&db_path).unwrap();
        let stats = db.get_stats().unwrap();

        assert_eq!(stats.total_symbols, 0);
        assert_eq!(stats.total_relationships, 0);
        assert_eq!(stats.total_files, 0);
    }

    #[test]
    fn test_minimal_database_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("minimal.db");

        // Test just the SQLite connection
        let conn = rusqlite::Connection::open(&db_path).unwrap();

        // Test a simple table creation
        let result = conn.execute(
            "CREATE TABLE test (id TEXT PRIMARY KEY, name TEXT)",
            []
        );

        // This should work without "Execute returned results" error
        assert!(result.is_ok());

        // Test a simple insert
        let insert_result = conn.execute(
            "INSERT INTO test VALUES ('1', 'test')",
            []
        );
        assert!(insert_result.is_ok());
    }

    #[test]
    fn test_debug_foreign_key_constraint() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("debug.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Create a temporary file
        let test_file = temp_dir.path().join("test.ts");
        std::fs::write(&test_file, "// test content").unwrap();

        // Store file info
        let file_info = crate::database::create_file_info(&test_file, "typescript").unwrap();
        println!("File path in file_info: {}", file_info.path);
        db.store_file_info(&file_info).unwrap();

        // Create a symbol with the same file path
        let file_path = test_file.to_string_lossy().to_string();
        println!("File path in symbol: {}", file_path);

        let symbol = Symbol {
            id: "test-symbol".to_string(),
            name: "testFunction".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: file_path,
            start_line: 1,
            start_column: 0,
            end_line: 1,
            end_column: 10,
            start_byte: 0,
            end_byte: 10,
            signature: None,
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        // This should work without foreign key constraint error
        let result = tokio::runtime::Runtime::new().unwrap().block_on(db.store_symbols(&[symbol]));
        assert!(result.is_ok(), "Foreign key constraint failed: {:?}", result);
    }

    #[test]
    fn test_individual_table_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("individual.db");

        // Create a SymbolDatabase instance manually to test each table individually
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let db = SymbolDatabase { conn, file_path: db_path };

        // Test files table creation
        let files_result = db.create_files_table();
        assert!(files_result.is_ok(), "Files table creation failed: {:?}", files_result);

        // Test symbols table creation
        let symbols_result = db.create_symbols_table();
        assert!(symbols_result.is_ok(), "Symbols table creation failed: {:?}", symbols_result);

        // Test relationships table creation
        let relationships_result = db.create_relationships_table();
        assert!(relationships_result.is_ok(), "Relationships table creation failed: {:?}", relationships_result);

        // Test embeddings table creation
        let embeddings_result = db.create_embeddings_table();
        assert!(embeddings_result.is_ok(), "Embeddings table creation failed: {:?}", embeddings_result);
    }

    #[tokio::test]
    async fn test_file_info_storage() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        let file_info = FileInfo {
            path: "test.rs".to_string(),
            language: "rust".to_string(),
            hash: "abcd1234".to_string(),
            size: 1024,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 5,
        };

        db.store_file_info(&file_info).unwrap();

        let hash = db.get_file_hash("test.rs").unwrap();
        assert_eq!(hash, Some("abcd1234".to_string()));
    }

    #[tokio::test]
    async fn test_symbol_storage_and_retrieval() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        let symbol = Symbol {
            id: "test-symbol-1".to_string(),
            name: "test_function".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "test.rs".to_string(),
            start_line: 10,
            start_column: 0,
            end_line: 15,
            end_column: 1,
            start_byte: 0,
            end_byte: 0,
            signature: Some("fn test_function()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        // Following foreign key contract: store file record first
        let file_info = FileInfo {
            path: "test.rs".to_string(),
            language: "rust".to_string(),
            hash: "test-hash".to_string(),
            size: 100,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
        };
        db.store_file_info(&file_info).unwrap();

        db.store_symbols(&[symbol.clone()]).await.unwrap();

        let retrieved = db.get_symbol_by_id("test-symbol-1").await.unwrap();
        assert!(retrieved.is_some());

        let retrieved_symbol = retrieved.unwrap();
        assert_eq!(retrieved_symbol.name, "test_function");
        assert_eq!(retrieved_symbol.language, "rust");
    }

    #[tokio::test]
    async fn test_symbol_with_metadata_and_semantic_fields() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Create a temporary file for the test
        let test_file = temp_dir.path().join("user.ts");
        std::fs::write(&test_file, "// test file content").unwrap();

        // Create symbol with all new fields populated
        let mut metadata = HashMap::new();
        metadata.insert("isAsync".to_string(), serde_json::Value::Bool(true));
        metadata.insert("returnType".to_string(), serde_json::Value::String("Promise<User>".to_string()));

        let symbol = Symbol {
            id: "test-symbol-complex".to_string(),
            name: "getUserAsync".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: test_file.to_string_lossy().to_string(),
            start_line: 20,
            start_column: 4,
            end_line: 30,
            end_column: 1,
            start_byte: 500,
            end_byte: 800,
            signature: Some("async getUserAsync(id: string): Promise<User>".to_string()),
            doc_comment: Some("Fetches user data asynchronously".to_string()),
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None, // No parent for this test
            metadata: Some(metadata.clone()),
            semantic_group: Some("user-data-access".to_string()),
            confidence: Some(0.95),
        };

        // First, store the file record (required due to foreign key constraint)
        let file_info = crate::database::create_file_info(&test_file, "typescript").unwrap();
        println!("DEBUG: File path in file_info: {}", file_info.path);
        println!("DEBUG: Symbol file path: {}", symbol.file_path);
        db.store_file_info(&file_info).unwrap();

        // Store the symbol
        db.store_symbols(&[symbol.clone()]).await.unwrap();

        // Retrieve and verify all fields are preserved
        let retrieved = db.get_symbol_by_id("test-symbol-complex").await.unwrap().unwrap();

        assert_eq!(retrieved.name, "getUserAsync");
        assert_eq!(retrieved.semantic_group, Some("user-data-access".to_string()));
        assert_eq!(retrieved.confidence, Some(0.95));

        // Verify metadata is properly stored and retrieved
        let retrieved_metadata = retrieved.metadata.unwrap();
        assert_eq!(retrieved_metadata.get("isAsync").unwrap().as_bool().unwrap(), true);
        assert_eq!(retrieved_metadata.get("returnType").unwrap().as_str().unwrap(), "Promise<User>");
    }

    #[tokio::test]
    async fn test_relationship_with_id_field() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Following foreign key contract: create file and symbols first
        let file_info = FileInfo {
            path: "main.rs".to_string(),
            language: "rust".to_string(),
            hash: "main-hash".to_string(),
            size: 500,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 2,
        };
        db.store_file_info(&file_info).unwrap();

        let caller_symbol = Symbol {
            id: "caller_func".to_string(),
            name: "caller_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "main.rs".to_string(),
            start_line: 10,
            start_column: 0,
            end_line: 15,
            end_column: 1,
            start_byte: 0,
            end_byte: 0,
            signature: Some("fn caller_func()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        let called_symbol = Symbol {
            id: "called_func".to_string(),
            name: "called_func".to_string(),
            kind: SymbolKind::Function,
            language: "rust".to_string(),
            file_path: "main.rs".to_string(),
            start_line: 20,
            start_column: 0,
            end_line: 25,
            end_column: 1,
            start_byte: 0,
            end_byte: 0,
            signature: Some("fn called_func()".to_string()),
            doc_comment: None,
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
        };

        db.store_symbols(&[caller_symbol, called_symbol]).await.unwrap();

        // Create relationship with generated id
        let relationship = crate::extractors::base::Relationship {
            id: "caller_func_called_func_Calls_42".to_string(),
            from_symbol_id: "caller_func".to_string(),
            to_symbol_id: "called_func".to_string(),
            kind: crate::extractors::base::RelationshipKind::Calls,
            file_path: "main.rs".to_string(),
            line_number: 42,
            confidence: 0.9,
            metadata: None,
        };

        // Store the relationship
        db.store_relationships(&[relationship.clone()]).await.unwrap();

        // Retrieve relationships for the from_symbol
        let relationships = db.get_relationships_for_symbol("caller_func").await.unwrap();
        assert_eq!(relationships.len(), 1);

        let retrieved = &relationships[0];
        assert_eq!(retrieved.id, "caller_func_called_func_Calls_42");
        assert_eq!(retrieved.from_symbol_id, "caller_func");
        assert_eq!(retrieved.to_symbol_id, "called_func");
        assert_eq!(retrieved.confidence, 0.9);
    }

    #[tokio::test]
    async fn test_cross_language_semantic_grouping() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Create symbols from different languages but same semantic group
        let ts_interface = Symbol {
            id: "ts-user-interface".to_string(),
            name: "User".to_string(),
            kind: SymbolKind::Interface,
            language: "typescript".to_string(),
            file_path: "user.ts".to_string(),
            start_line: 1,
            start_column: 0,
            end_line: 10,
            end_column: 1,
            start_byte: 0,
            end_byte: 200,
            signature: Some("interface User".to_string()),
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None,
            metadata: None,
            semantic_group: Some("user-entity".to_string()),
            confidence: Some(1.0),
        };

        let rust_struct = Symbol {
            id: "rust-user-struct".to_string(),
            name: "User".to_string(),
            kind: SymbolKind::Struct,
            language: "rust".to_string(),
            file_path: "user.rs".to_string(),
            start_line: 5,
            start_column: 0,
            end_line: 15,
            end_column: 1,
            start_byte: 100,
            end_byte: 400,
            signature: Some("struct User".to_string()),
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None,
            metadata: None,
            semantic_group: Some("user-entity".to_string()),
            confidence: Some(0.98),
        };

        // Following foreign key contract: store file records first
        let ts_file_info = FileInfo {
            path: "user.ts".to_string(),
            language: "typescript".to_string(),
            hash: "ts-hash".to_string(),
            size: 200,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
        };
        db.store_file_info(&ts_file_info).unwrap();

        let rust_file_info = FileInfo {
            path: "user.rs".to_string(),
            language: "rust".to_string(),
            hash: "rust-hash".to_string(),
            size: 300,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
        };
        db.store_file_info(&rust_file_info).unwrap();

        // Store both symbols
        db.store_symbols(&[ts_interface, rust_struct]).await.unwrap();

        // Query symbols by semantic group (this will fail initially - need to implement)
        let grouped_symbols = db.get_symbols_by_semantic_group("user-entity").await.unwrap();
        assert_eq!(grouped_symbols.len(), 2);

        // Verify we have both TypeScript and Rust symbols
        let languages: std::collections::HashSet<_> = grouped_symbols.iter()
            .map(|s| s.language.as_str())
            .collect();
        assert!(languages.contains("typescript"));
        assert!(languages.contains("rust"));
    }

    #[tokio::test]
    async fn test_extractor_database_integration() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = SymbolDatabase::new(&db_path).unwrap();

        // Simulate what an extractor would create
        use crate::extractors::base::BaseExtractor;

        let source_code = r#"
        function getUserById(id: string): Promise<User> {
            return fetchUser(id);
        }
        "#;

        // This test will initially fail - we need to verify extractors can create symbols
        // with the new field structure that work with the database
        let base_extractor = BaseExtractor::new("typescript".to_string(), "test.ts".to_string(), source_code.to_string());

        // Create a symbol like an extractor would
        let mut metadata = HashMap::new();
        metadata.insert("isAsync".to_string(), serde_json::Value::Bool(false));
        metadata.insert("returnType".to_string(), serde_json::Value::String("Promise<User>".to_string()));

        let symbol = Symbol {
            id: base_extractor.generate_id("getUserById", 2, 8),
            name: "getUserById".to_string(),
            kind: SymbolKind::Function,
            language: "typescript".to_string(),
            file_path: "test.ts".to_string(),
            start_line: 2,
            start_column: 8,
            end_line: 4,
            end_column: 9,
            start_byte: 0,
            end_byte: 0,
            signature: Some("function getUserById(id: string): Promise<User>".to_string()),
            doc_comment: None,
            visibility: Some(crate::extractors::base::Visibility::Public),
            parent_id: None,
            metadata: Some(metadata),
            semantic_group: None, // Will be populated during cross-language analysis
            confidence: None, // Will be calculated based on parsing context
        };

        // Following foreign key contract: store file record first
        let file_info = FileInfo {
            path: "test.ts".to_string(),
            language: "typescript".to_string(),
            hash: "test-ts-hash".to_string(),
            size: 150,
            last_modified: 1234567890,
            last_indexed: 0,
            symbol_count: 1,
        };
        db.store_file_info(&file_info).unwrap();

        // Test that extractor-generated symbols work with database
        db.store_symbols(&[symbol.clone()]).await.unwrap();

        let retrieved = db.get_symbol_by_id(&symbol.id).await.unwrap().unwrap();
        assert_eq!(retrieved.name, "getUserById");
        assert!(retrieved.metadata.is_some());

        let metadata = retrieved.metadata.unwrap();
        assert_eq!(metadata.get("returnType").unwrap().as_str().unwrap(), "Promise<User>");
    }
}