// Helper functions and utilities

use super::*;
use anyhow::Result;

/// Standard SELECT column list for Symbol queries
/// CRITICAL: This must stay in sync with row_to_symbol() expectations
/// When adding a new column to Symbol struct, update BOTH:
/// 1. This constant (add column to SELECT list)
/// 2. row_to_symbol() (add row.get() call)
pub(crate) const SYMBOL_COLUMNS: &str = "id, name, kind, language, file_path, signature, \
     start_line, start_col, end_line, end_col, start_byte, end_byte, \
     doc_comment, visibility, code_context, parent_id, \
     metadata, semantic_group, confidence, content_type";

impl SymbolDatabase {
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
        let total_symbols: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        let total_relationships: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;

        let total_files: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;

        // Get unique languages
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT language FROM files ORDER BY language")?;

        let language_iter = stmt.query_map([], |row| row.get::<_, String>(0))?;

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
            languages,
            db_size_mb,
        })
    }

    /// Helper to convert database row to Symbol
    pub(crate) fn row_to_symbol(&self, row: &Row) -> rusqlite::Result<Symbol> {
        let kind_str: String = row.get("kind")?;
        let kind = SymbolKind::from_string(&kind_str);

        let metadata_json: Option<String> = row.get("metadata")?;
        let metadata = metadata_json.and_then(|json| serde_json::from_str(&json).ok());

        // Deserialize visibility string to enum
        let visibility_str: Option<String> = row.get("visibility")?;
        let visibility = visibility_str.and_then(|v| match v.as_str() {
            "public" => Some(crate::extractors::base::Visibility::Public),
            "private" => Some(crate::extractors::base::Visibility::Private),
            "protected" => Some(crate::extractors::base::Visibility::Protected),
            _ => None,
        });

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
            start_byte: row.get("start_byte")?,
            end_byte: row.get("end_byte")?,
            doc_comment: row.get("doc_comment")?,
            visibility,
            parent_id: row.get("parent_id")?,
            metadata,
            semantic_group: row.get("semantic_group")?,
            confidence: row.get("confidence")?,
            code_context: row.get("code_context")?,
            content_type: row.get("content_type")?,
        })
    }

    /// Helper to convert database row to Relationship
    pub(crate) fn row_to_relationship(&self, row: &Row) -> rusqlite::Result<Relationship> {
        let kind_str: String = row.get("kind")?;
        let kind = RelationshipKind::from_string(&kind_str);

        let metadata_json: Option<String> = row.get("metadata")?;
        let metadata = metadata_json.and_then(|json| serde_json::from_str(&json).ok());

        Ok(Relationship {
            id: row.get("id")?,
            from_symbol_id: row.get("from_symbol_id")?,
            to_symbol_id: row.get("to_symbol_id")?,
            kind,
            file_path: row.get("file_path").unwrap_or_else(|_| String::new()),
            line_number: row.get("line_number").unwrap_or(0),
            confidence: row.get("confidence").unwrap_or(1.0),
            metadata,
        })
    }
}
