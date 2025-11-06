//! Knowledge embeddings database schema for RAG system
//!
//! This module implements the unified knowledge embeddings architecture that supports:
//! - Documentation sections (markdown docs, READMEs, architecture docs)
//! - Code symbols (functions, classes, methods)
//! - Test cases (unit tests, integration tests)
//! - Architecture Decision Records (ADRs)
//! - Code comments and doc comments
//!
//! The unified schema enables cross-domain semantic search and relationship discovery.

use anyhow::Result;
use tracing::debug;

use super::SymbolDatabase;

impl SymbolDatabase {
    /// Create the knowledge embeddings table for unified RAG
    ///
    /// This table stores embeddings for all types of knowledge entities:
    /// - code_symbol: Functions, classes, variables (from symbols table)
    /// - doc_section: Markdown documentation sections
    /// - test_case: Unit and integration tests
    /// - adr: Architecture Decision Records
    /// - comment: Code comments and doc comments
    ///
    /// The unified design enables:
    /// - Cross-domain semantic search (find docs related to code)
    /// - Relationship discovery (link implementations to decisions)
    /// - Token-efficient context retrieval (get only relevant chunks)
    pub(crate) fn create_knowledge_embeddings_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_embeddings (
                id INTEGER PRIMARY KEY AUTOINCREMENT,

                -- Entity identification
                entity_type TEXT NOT NULL CHECK(entity_type IN (
                    'code_symbol',
                    'doc_section',
                    'test_case',
                    'adr',
                    'comment'
                )),
                entity_id TEXT NOT NULL,

                -- Source information
                source_file TEXT NOT NULL,
                section_title TEXT,
                language TEXT,

                -- Content
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,

                -- Embedding reference (reuses existing vector storage)
                vector_id TEXT NOT NULL,
                model_name TEXT NOT NULL,

                -- Metadata (JSON for flexibility)
                metadata TEXT,

                -- Timestamps
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,

                -- Constraints
                UNIQUE(entity_type, entity_id, model_name),
                FOREIGN KEY (vector_id) REFERENCES embedding_vectors(vector_id) ON DELETE CASCADE
            )",
            [],
        )?;

        // Indexes for common query patterns
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_entity_type
            ON knowledge_embeddings(entity_type)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_source_file
            ON knowledge_embeddings(source_file)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_model_name
            ON knowledge_embeddings(model_name)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_content_hash
            ON knowledge_embeddings(content_hash)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_vector_id
            ON knowledge_embeddings(vector_id)",
            [],
        )?;

        debug!("Created knowledge_embeddings table with indexes");
        Ok(())
    }

    /// Create the knowledge relationships table for explicit cross-references
    ///
    /// This table captures explicit relationships between knowledge entities:
    /// - implements: Code implements an ADR or design decision
    /// - documents: Documentation describes code
    /// - tests: Test case validates a function/class
    /// - decides: ADR makes a decision about architecture
    /// - references: General reference relationship
    pub(crate) fn create_knowledge_relationships_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_relationships (
                id INTEGER PRIMARY KEY AUTOINCREMENT,

                from_id INTEGER NOT NULL,
                to_id INTEGER NOT NULL,

                relationship_type TEXT NOT NULL CHECK(relationship_type IN (
                    'implements',
                    'documents',
                    'tests',
                    'decides',
                    'references'
                )),

                confidence REAL DEFAULT 1.0 CHECK(confidence >= 0.0 AND confidence <= 1.0),

                created_at INTEGER NOT NULL,

                FOREIGN KEY (from_id) REFERENCES knowledge_embeddings(id) ON DELETE CASCADE,
                FOREIGN KEY (to_id) REFERENCES knowledge_embeddings(id) ON DELETE CASCADE
            )",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_rel_from
            ON knowledge_relationships(from_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_rel_to
            ON knowledge_relationships(to_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_knowledge_rel_type
            ON knowledge_relationships(relationship_type)",
            [],
        )?;

        debug!("Created knowledge_relationships table with indexes");
        Ok(())
    }

    /// Create FTS5 virtual table for full-text search on knowledge content
    ///
    /// This enables fast keyword search across all knowledge entities,
    /// complementing semantic search with traditional text search.
    pub(crate) fn create_knowledge_fts_table(&self) -> Result<()> {
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_fts USING fts5(
                entity_id UNINDEXED,
                section_title,
                content,
                content='knowledge_embeddings',
                content_rowid='id',
                tokenize='porter unicode61'
            )",
            [],
        )?;

        debug!("Created knowledge_fts virtual table");
        Ok(())
    }

    /// Create triggers to keep knowledge_fts in sync with knowledge_embeddings
    pub(crate) fn create_knowledge_fts_triggers(&self) -> Result<()> {
        // Insert trigger
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS knowledge_fts_insert
            AFTER INSERT ON knowledge_embeddings
            BEGIN
                INSERT INTO knowledge_fts(rowid, entity_id, section_title, content)
                VALUES (new.id, new.entity_id, new.section_title, new.content);
            END",
            [],
        )?;

        // Update trigger
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS knowledge_fts_update
            AFTER UPDATE ON knowledge_embeddings
            BEGIN
                UPDATE knowledge_fts
                SET entity_id = new.entity_id,
                    section_title = new.section_title,
                    content = new.content
                WHERE rowid = new.id;
            END",
            [],
        )?;

        // Delete trigger
        self.conn.execute(
            "CREATE TRIGGER IF NOT EXISTS knowledge_fts_delete
            AFTER DELETE ON knowledge_embeddings
            BEGIN
                DELETE FROM knowledge_fts WHERE rowid = old.id;
            END",
            [],
        )?;

        debug!("Created knowledge_fts triggers");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_knowledge_schema() -> Result<()> {
        use rusqlite::Connection;
        use std::path::PathBuf;

        let conn = Connection::open_in_memory()?;
        let db = SymbolDatabase {
            conn,
            file_path: PathBuf::from(":memory:"),
        };

        // Need embedding_vectors table first (foreign key dependency)
        db.conn.execute(
            "CREATE TABLE embedding_vectors (
                vector_id TEXT PRIMARY KEY,
                dimensions INTEGER NOT NULL,
                vector_data BLOB NOT NULL,
                model_name TEXT NOT NULL,
                created_at INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Create knowledge tables
        db.create_knowledge_embeddings_table()?;
        db.create_knowledge_relationships_table()?;
        db.create_knowledge_fts_table()?;
        db.create_knowledge_fts_triggers()?;

        // Verify tables exist
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_embeddings'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);

        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='knowledge_relationships'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1);

        Ok(())
    }

    #[test]
    fn test_knowledge_entity_type_constraint() -> Result<()> {
        use rusqlite::Connection;
        use std::path::PathBuf;

        let conn = Connection::open_in_memory()?;
        let db = SymbolDatabase {
            conn,
            file_path: PathBuf::from(":memory:"),
        };

        // Create dependencies
        db.conn.execute(
            "CREATE TABLE embedding_vectors (
                vector_id TEXT PRIMARY KEY,
                dimensions INTEGER NOT NULL,
                vector_data BLOB NOT NULL,
                model_name TEXT NOT NULL,
                created_at INTEGER DEFAULT 0
            )",
            [],
        )?;

        db.create_knowledge_embeddings_table()?;

        // Insert test vector
        db.conn.execute(
            "INSERT INTO embedding_vectors (vector_id, dimensions, vector_data, model_name, created_at)
            VALUES ('test_vec', 384, X'', 'bge-small', 0)",
            [],
        )?;

        // Valid entity type should succeed
        let result = db.conn.execute(
            "INSERT INTO knowledge_embeddings
            (entity_type, entity_id, source_file, content, content_hash, vector_id, model_name, created_at, updated_at)
            VALUES ('doc_section', 'test_doc', 'README.md', 'Test content', 'hash123', 'test_vec', 'bge-small', 0, 0)",
            [],
        );
        assert!(result.is_ok());

        // Invalid entity type should fail
        let result = db.conn.execute(
            "INSERT INTO knowledge_embeddings
            (entity_type, entity_id, source_file, content, content_hash, vector_id, model_name, created_at, updated_at)
            VALUES ('invalid_type', 'test_doc', 'README.md', 'Test content', 'hash123', 'test_vec', 'bge-small', 0, 0)",
            [],
        );
        assert!(result.is_err());

        Ok(())
    }

    #[test]
    fn test_knowledge_relationship_constraint() -> Result<()> {
        use rusqlite::Connection;
        use std::path::PathBuf;

        let conn = Connection::open_in_memory()?;
        let db = SymbolDatabase {
            conn,
            file_path: PathBuf::from(":memory:"),
        };

        // Create dependencies
        db.conn.execute(
            "CREATE TABLE embedding_vectors (
                vector_id TEXT PRIMARY KEY,
                dimensions INTEGER NOT NULL,
                vector_data BLOB NOT NULL,
                model_name TEXT NOT NULL,
                created_at INTEGER DEFAULT 0
            )",
            [],
        )?;

        db.create_knowledge_embeddings_table()?;
        db.create_knowledge_relationships_table()?;

        // Insert test vector
        db.conn.execute(
            "INSERT INTO embedding_vectors (vector_id, dimensions, vector_data, model_name, created_at)
            VALUES ('test_vec', 384, X'', 'bge-small', 0)",
            [],
        )?;

        // Insert test entities
        db.conn.execute(
            "INSERT INTO knowledge_embeddings
            (entity_type, entity_id, source_file, content, content_hash, vector_id, model_name, created_at, updated_at)
            VALUES ('doc_section', 'doc1', 'README.md', 'Test', 'hash1', 'test_vec', 'bge-small', 0, 0)",
            [],
        )?;

        db.conn.execute(
            "INSERT INTO knowledge_embeddings
            (entity_type, entity_id, source_file, content, content_hash, vector_id, model_name, created_at, updated_at)
            VALUES ('code_symbol', 'func1', 'main.rs', 'fn test() {}', 'hash2', 'test_vec', 'bge-small', 0, 0)",
            [],
        )?;

        // Valid relationship type should succeed
        let result = db.conn.execute(
            "INSERT INTO knowledge_relationships (from_id, to_id, relationship_type, created_at)
            VALUES (1, 2, 'documents', 0)",
            [],
        );
        assert!(result.is_ok());

        // Invalid relationship type should fail
        let result = db.conn.execute(
            "INSERT INTO knowledge_relationships (from_id, to_id, relationship_type, created_at)
            VALUES (1, 2, 'invalid_type', 0)",
            [],
        );
        assert!(result.is_err());

        Ok(())
    }
}
