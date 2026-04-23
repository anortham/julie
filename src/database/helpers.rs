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

/// Lightweight SELECT column list — skips expensive columns that are unused in structure mode.
/// Omits: code_context (large, immediately discarded), metadata (expensive JSON parse),
/// semantic_group, confidence, content_type (unused in filtering/formatting).
/// CRITICAL: Must stay in sync with row_to_symbol_lightweight() expectations.
pub(crate) const SYMBOL_COLUMNS_LIGHTWEIGHT: &str = "id, name, kind, language, file_path, signature, \
     start_line, start_col, end_line, end_col, start_byte, end_byte, \
     doc_comment, visibility, parent_id";

pub(crate) const SYMBOL_UPSERT_SQL: &str = "INSERT INTO symbols
     (id, name, kind, language, file_path, signature, start_line, start_col,
      end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
      parent_id, metadata, semantic_group, confidence, content_type)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
     ON CONFLICT(id) DO UPDATE SET
      name = excluded.name,
      kind = excluded.kind,
      language = excluded.language,
      file_path = excluded.file_path,
      signature = excluded.signature,
      start_line = excluded.start_line,
      start_col = excluded.start_col,
      end_line = excluded.end_line,
      end_col = excluded.end_col,
      start_byte = excluded.start_byte,
      end_byte = excluded.end_byte,
      doc_comment = excluded.doc_comment,
      visibility = excluded.visibility,
      code_context = excluded.code_context,
      parent_id = excluded.parent_id,
      metadata = excluded.metadata,
      semantic_group = excluded.semantic_group,
      confidence = excluded.confidence,
      content_type = excluded.content_type,
      file_hash = NULL,
      last_indexed = 0,
      reference_score = 0.0";

impl SymbolDatabase {
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
            embedding_count: self.embedding_count().unwrap_or(0),
        })
    }

    /// Count files grouped by language, sorted by count descending.
    pub fn count_files_by_language(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT language, COUNT(*) as cnt FROM files GROUP BY language ORDER BY cnt DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Count symbols grouped by kind, sorted by count descending.
    pub fn count_symbols_by_kind(&self) -> Result<Vec<(String, i64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT kind, COUNT(*) as cnt FROM symbols GROUP BY kind ORDER BY cnt DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
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
            annotations: Vec::new(),
        })
    }

    /// Lightweight row mapper — skips expensive columns not in SYMBOL_COLUMNS_LIGHTWEIGHT.
    /// Sets code_context, metadata, semantic_group, confidence, content_type to None.
    pub(crate) fn row_to_symbol_lightweight(&self, row: &Row) -> rusqlite::Result<Symbol> {
        let kind_str: String = row.get("kind")?;
        let kind = SymbolKind::from_string(&kind_str);

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
            metadata: None,
            semantic_group: None,
            confidence: None,
            code_context: None,
            content_type: None,
            annotations: Vec::new(),
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
            file_path: row.get("file_path")?,
            line_number: row.get("line_number")?,
            confidence: row.get::<_, Option<f64>>("confidence")?.unwrap_or(1.0) as f32,
            metadata,
        })
    }
}
