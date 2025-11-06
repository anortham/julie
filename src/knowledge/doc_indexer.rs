/// Documentation indexer - Identifies and processes documentation symbols
///
/// This module determines which symbols represent documentation (markdown files)
/// vs code/configuration, enabling RAG semantic search over docs.

use crate::database::SymbolDatabase;
use crate::extractors::base::Symbol;
use anyhow::{Context, Result};
use rusqlite::params;
use sha2::{Digest, Sha256};

pub struct DocumentationIndexer;

impl DocumentationIndexer {
    /// Determine if a symbol represents documentation
    ///
    /// Returns true if the symbol is from a markdown file, which we treat as documentation.
    /// JSON and TOML files are configuration, not documentation.
    ///
    /// # Examples
    /// - `docs/CLAUDE.md` → true (documentation)
    /// - `README.md` → true (documentation)
    /// - `package.json` → false (configuration)
    /// - `src/main.rs` → false (code)
    pub fn is_documentation_symbol(symbol: &Symbol) -> bool {
        // Documentation is identified by file extension
        // Markdown files (.md, .markdown) are documentation
        // JSON and TOML are configuration files, not documentation
        let path_lower = symbol.file_path.to_lowercase();
        path_lower.ends_with(".md") || path_lower.ends_with(".markdown")
    }

    /// Store documentation symbol as knowledge embedding
    ///
    /// This function stores a documentation symbol (markdown section) in the knowledge_embeddings
    /// table for RAG semantic search. It:
    /// - Validates the symbol is documentation (returns early if not)
    /// - Computes content from symbol name and doc_comment
    /// - Generates content hash for deduplication
    /// - Creates a dummy vector_id (actual embeddings will be generated later)
    /// - Inserts into knowledge_embeddings with entity_type='doc_section'
    ///
    /// # Arguments
    /// * `db` - Database connection
    /// * `symbol` - Documentation symbol to store
    /// * `model_name` - Embedding model name (e.g., "bge-small-en")
    ///
    /// # Returns
    /// * `Ok(())` if stored successfully or skipped (non-documentation)
    /// * `Err` if database operation fails
    pub fn store_documentation_embedding(
        db: &SymbolDatabase,
        symbol: &Symbol,
        model_name: &str,
    ) -> Result<()> {
        // Skip non-documentation symbols
        if !Self::is_documentation_symbol(symbol) {
            return Ok(()); // Not an error, just not documentation
        }

        // Build content from symbol name and doc_comment
        let content = if let Some(doc) = &symbol.doc_comment {
            format!("{}\n\n{}", symbol.name, doc)
        } else {
            symbol.name.clone()
        };

        // Compute content hash for deduplication
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let content_hash = format!("{:x}", hasher.finalize());

        // Use symbol.id as entity_id
        let entity_id = &symbol.id;

        // Create dummy vector_id (embeddings will be generated in a later step)
        // For now, use entity_id as vector_id
        let vector_id = entity_id.clone();

        // Get current timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Failed to get system time")?
            .as_secs() as i64;

        // First, ensure dummy vector exists (needed for FK constraint)
        // In production, this would be replaced with actual embedding generation
        db.conn.execute(
            "INSERT OR IGNORE INTO embedding_vectors (vector_id, dimensions, vector_data, model_name, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                &vector_id,
                384_i64, // BGE-small dimensions
                vec![0u8], // Dummy empty vector for now
                model_name,
                now
            ],
        )?;

        // Insert into knowledge_embeddings table
        db.conn.execute(
            "INSERT OR REPLACE INTO knowledge_embeddings
             (entity_type, entity_id, source_file, section_title, language, content, content_hash, vector_id, model_name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                "doc_section",
                entity_id,
                &symbol.file_path,
                &symbol.name, // Section title from symbol name
                &symbol.language,
                &content,
                &content_hash,
                &vector_id,
                model_name,
                now,
                now
            ],
        ).context("Failed to insert documentation embedding")?;

        Ok(())
    }
}
