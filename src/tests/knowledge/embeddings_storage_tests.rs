// Knowledge Embeddings Storage Tests
// Following TDD methodology: RED -> GREEN -> REFACTOR

#[cfg(test)]
mod knowledge_embeddings_storage_tests {
    use crate::database::SymbolDatabase;
    use crate::extractors::base::{Symbol, SymbolKind};
    use crate::knowledge::doc_indexer::DocumentationIndexer;
    use anyhow::Result;
    use rusqlite::Connection;
    use std::path::PathBuf;

    fn create_test_symbol(file_path: &str, name: &str, content: &str) -> Symbol {
        Symbol {
            id: format!("test_{}", name),
            name: name.to_string(),
            kind: SymbolKind::Module,
            file_path: file_path.to_string(),
            language: "markdown".to_string(),
            start_line: 1,
            end_line: 10,
            start_column: 0,
            end_column: 0,
            start_byte: 0,
            end_byte: 100,
            signature: None,
            doc_comment: Some(content.to_string()),
            visibility: None,
            parent_id: None,
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
        }
    }

    fn setup_test_db() -> Result<SymbolDatabase> {
        let conn = Connection::open_in_memory()?;
        let db = SymbolDatabase {
            conn,
            file_path: PathBuf::from(":memory:"),
        };

        // Create dependencies - embedding_vectors table
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

        // Create knowledge embeddings schema
        db.create_knowledge_embeddings_table()?;
        db.create_knowledge_fts_table()?;
        db.create_knowledge_fts_triggers()?;

        Ok(db)
    }

    #[test]
    fn test_store_documentation_embedding_basic() -> Result<()> {
        let db = setup_test_db()?;
        let symbol = create_test_symbol(
            "docs/ARCHITECTURE.md",
            "System Overview",
            "This document describes the overall system architecture.",
        );

        // Store documentation embedding
        DocumentationIndexer::store_documentation_embedding(&db, &symbol, "bge-small-en")?;

        // Verify stored in knowledge_embeddings table
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM knowledge_embeddings WHERE entity_type = 'doc_section'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(count, 1, "Should have 1 doc_section in knowledge_embeddings");

        // Verify entity details
        let (entity_id, source_file, content): (String, String, String) = db.conn.query_row(
            "SELECT entity_id, source_file, content FROM knowledge_embeddings WHERE entity_type = 'doc_section'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;

        assert_eq!(entity_id, "test_System Overview");
        assert_eq!(source_file, "docs/ARCHITECTURE.md");
        assert!(
            content.contains("System Overview"),
            "Content should include section title"
        );

        Ok(())
    }

    #[test]
    fn test_store_documentation_embedding_with_section_title() -> Result<()> {
        let db = setup_test_db()?;
        let symbol = create_test_symbol(
            "README.md",
            "Installation",
            "Run `cargo install` to install this tool.",
        );

        DocumentationIndexer::store_documentation_embedding(&db, &symbol, "bge-small-en")?;

        // Verify section_title is stored
        let section_title: Option<String> = db.conn.query_row(
            "SELECT section_title FROM knowledge_embeddings WHERE entity_type = 'doc_section'",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(section_title, Some("Installation".to_string()));

        Ok(())
    }

    #[test]
    fn test_store_documentation_embedding_deduplication() -> Result<()> {
        let db = setup_test_db()?;
        let symbol = create_test_symbol(
            "docs/GUIDE.md",
            "Getting Started",
            "Welcome to the guide.",
        );

        // Store same documentation twice
        DocumentationIndexer::store_documentation_embedding(&db, &symbol, "bge-small-en")?;
        DocumentationIndexer::store_documentation_embedding(&db, &symbol, "bge-small-en")?;

        // Should still have only 1 entry (UNIQUE constraint on entity_type, entity_id, model_name)
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM knowledge_embeddings WHERE entity_type = 'doc_section'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(
            count, 1,
            "Duplicate documentation should be deduplicated by UNIQUE constraint"
        );

        Ok(())
    }

    #[test]
    fn test_store_documentation_embedding_content_hash() -> Result<()> {
        let db = setup_test_db()?;
        let symbol = create_test_symbol(
            "docs/API.md",
            "Endpoints",
            "List of all available API endpoints.",
        );

        DocumentationIndexer::store_documentation_embedding(&db, &symbol, "bge-small-en")?;

        // Verify content_hash is computed and stored
        let content_hash: String = db.conn.query_row(
            "SELECT content_hash FROM knowledge_embeddings WHERE entity_type = 'doc_section'",
            [],
            |row| row.get(0),
        )?;

        assert!(
            !content_hash.is_empty(),
            "Content hash should be computed and stored"
        );

        Ok(())
    }

    #[test]
    fn test_store_documentation_embedding_fts_sync() -> Result<()> {
        let db = setup_test_db()?;
        let symbol = create_test_symbol(
            "docs/DEPLOY.md",
            "Deployment",
            "Instructions for deploying to production.",
        );

        DocumentationIndexer::store_documentation_embedding(&db, &symbol, "bge-small-en")?;

        // Verify FTS5 table is automatically synced via trigger
        let count: i64 = db.conn.query_row(
            "SELECT COUNT(*) FROM knowledge_fts WHERE content MATCH 'deployment'",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(
            count, 1,
            "FTS5 table should be synced via trigger for full-text search"
        );

        Ok(())
    }

    #[test]
    fn test_store_documentation_embedding_ignores_non_docs() -> Result<()> {
        let db = setup_test_db()?;
        let mut code_symbol = create_test_symbol("src/main.rs", "main", "fn main() {}");
        code_symbol.language = "rust".to_string();

        // Should return error or skip storing (implementation detail)
        let result = DocumentationIndexer::store_documentation_embedding(&db, &code_symbol, "bge-small-en");

        // Either error or no-op is acceptable
        if result.is_ok() {
            // If no error, verify nothing was stored
            let count: i64 = db.conn.query_row(
                "SELECT COUNT(*) FROM knowledge_embeddings WHERE entity_type = 'doc_section'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(count, 0, "Non-documentation symbols should not be stored");
        }

        Ok(())
    }

    #[test]
    fn test_store_documentation_embedding_metadata() -> Result<()> {
        let db = setup_test_db()?;
        let symbol = create_test_symbol(
            "docs/CHANGELOG.md",
            "v1.0.0",
            "Initial release with core features.",
        );

        DocumentationIndexer::store_documentation_embedding(&db, &symbol, "bge-small-en")?;

        // Verify language is stored in metadata or as column
        let language: String = db.conn.query_row(
            "SELECT language FROM knowledge_embeddings WHERE entity_type = 'doc_section'",
            [],
            |row| row.get(0),
        )?;

        assert_eq!(language, "markdown");

        Ok(())
    }
}
