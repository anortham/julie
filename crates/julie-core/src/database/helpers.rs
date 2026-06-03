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
     metadata, semantic_group, confidence, content_type, \
     body_start_line, body_start_col, body_end_line, body_end_col, \
     body_start_byte, body_end_byte, body_hash";

/// Lightweight SELECT column list — skips expensive columns that are unused in structure mode.
/// Omits: code_context (large, immediately discarded), metadata (expensive JSON parse),
/// semantic_group, confidence, content_type (unused in filtering/formatting).
/// CRITICAL: Must stay in sync with row_to_symbol_lightweight() expectations.
pub(crate) const SYMBOL_COLUMNS_LIGHTWEIGHT: &str = "id, name, kind, language, file_path, signature, \
     start_line, start_col, end_line, end_col, start_byte, end_byte, \
     doc_comment, visibility, parent_id, \
     body_start_line, body_start_col, body_end_line, body_end_col, \
     body_start_byte, body_end_byte, body_hash";

pub(crate) const SYMBOL_UPSERT_SQL: &str = "INSERT INTO symbols
     (id, name, kind, language, file_path, signature, start_line, start_col,
      end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
      parent_id, metadata, semantic_group, confidence, content_type,
      body_start_line, body_start_col, body_end_line, body_end_col,
      body_start_byte, body_end_byte, body_hash)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
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
      body_start_line = excluded.body_start_line,
      body_start_col = excluded.body_start_col,
      body_end_line = excluded.body_end_line,
      body_end_col = excluded.body_end_col,
      body_start_byte = excluded.body_start_byte,
      body_end_byte = excluded.body_end_byte,
      body_hash = excluded.body_hash,
      file_hash = NULL,
      last_indexed = 0,
      reference_score = 0.0";

fn row_conversion_error(column: usize, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn parse_symbol_kind(kind: &str) -> rusqlite::Result<SymbolKind> {
    SymbolKind::try_from_string(kind)
        .ok_or_else(|| row_conversion_error(2, format!("unknown symbol kind: {kind}")))
}

fn parse_symbol_visibility(
    visibility: &str,
) -> rusqlite::Result<julie_extractors::base::Visibility> {
    julie_extractors::base::Visibility::from_storage_str(visibility)
        .ok_or_else(|| row_conversion_error(13, format!("unknown symbol visibility: {visibility}")))
}

fn parse_relationship_kind(kind: &str) -> rusqlite::Result<RelationshipKind> {
    RelationshipKind::try_from_string(kind)
        .ok_or_else(|| row_conversion_error(3, format!("unknown relationship kind: {kind}")))
}

fn row_to_body_span(
    row: &Row,
) -> rusqlite::Result<Option<julie_extractors::base::NormalizedSpan>> {
    let start_line: Option<u32> = row.get("body_start_line")?;
    let start_column: Option<u32> = row.get("body_start_col")?;
    let end_line: Option<u32> = row.get("body_end_line")?;
    let end_column: Option<u32> = row.get("body_end_col")?;
    let start_byte: Option<u32> = row.get("body_start_byte")?;
    let end_byte: Option<u32> = row.get("body_end_byte")?;

    match (
        start_line,
        start_column,
        end_line,
        end_column,
        start_byte,
        end_byte,
    ) {
        (None, None, None, None, None, None) => Ok(None),
        (
            Some(start_line),
            Some(start_column),
            Some(end_line),
            Some(end_column),
            Some(start_byte),
            Some(end_byte),
        ) => Ok(Some(julie_extractors::base::NormalizedSpan {
            start_line,
            start_column,
            end_line,
            end_column,
            start_byte,
            end_byte,
        })),
        _ => Err(row_conversion_error(
            20,
            "incomplete symbol body span columns".to_string(),
        )),
    }
}

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
        let kind = parse_symbol_kind(&kind_str)?;

        let metadata_json: Option<String> = row.get("metadata")?;
        let metadata = metadata_json.and_then(|json| serde_json::from_str(&json).ok());

        // Deserialize visibility string to enum
        let visibility_str: Option<String> = row.get("visibility")?;
        let visibility = visibility_str
            .as_deref()
            .map(parse_symbol_visibility)
            .transpose()?;

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
            body_span: row_to_body_span(row)?,
            body_hash: row.get("body_hash")?,
            annotations: Vec::new(),
        })
    }

    /// Lightweight row mapper — skips expensive columns not in SYMBOL_COLUMNS_LIGHTWEIGHT.
    /// Sets code_context, metadata, semantic_group, confidence, content_type to None.
    pub(crate) fn row_to_symbol_lightweight(&self, row: &Row) -> rusqlite::Result<Symbol> {
        let kind_str: String = row.get("kind")?;
        let kind = parse_symbol_kind(&kind_str)?;

        let visibility_str: Option<String> = row.get("visibility")?;
        let visibility = visibility_str
            .as_deref()
            .map(parse_symbol_visibility)
            .transpose()?;

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
            body_span: row_to_body_span(row)?,
            body_hash: row.get("body_hash")?,
            annotations: Vec::new(),
        })
    }

    /// Helper to convert database row to Relationship
    pub(crate) fn row_to_relationship(&self, row: &Row) -> rusqlite::Result<Relationship> {
        let kind_str: String = row.get("kind")?;
        let kind = parse_relationship_kind(&kind_str)?;

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
