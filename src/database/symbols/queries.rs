// Symbol query operations

use super::super::helpers::{SYMBOL_COLUMNS, SYMBOL_COLUMNS_LIGHTWEIGHT};
use super::super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::debug;

impl SymbolDatabase {
    pub fn get_symbol_by_id(&self, id: &str) -> Result<Option<Symbol>> {
        let query = format!("SELECT {} FROM symbols WHERE id = ?1", SYMBOL_COLUMNS);
        let mut stmt = self.conn.prepare(&query)?;

        let result = stmt.query_row(params![id], |row| self.row_to_symbol(row));

        match result {
            Ok(symbol) => Ok(Some(symbol)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Get multiple symbols by their IDs in one batched query (for batched lookups)
    ///
    /// **CRITICAL**: Preserves the input order of IDs in the returned results.
    /// This is essential for search where relevance scores must match their corresponding symbols.
    pub fn get_symbols_by_ids(&self, ids: &[String]) -> Result<Vec<Symbol>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause for batch fetch
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();

        // Build CASE statement for ORDER BY to preserve input order
        // This maps each ID to its position in the input array
        let order_cases: Vec<String> = (0..ids.len())
            .map(|i| format!("WHEN id = ?{} THEN {}", i + 1, i))
            .collect();

        let query = format!(
            "SELECT {} FROM symbols WHERE id IN ({}) ORDER BY CASE {} END",
            SYMBOL_COLUMNS,
            placeholders.join(", "),
            order_cases.join(" ")
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Convert Vec<String> to Vec<&dyn ToSql> for params!
        let params: Vec<&dyn rusqlite::ToSql> =
            ids.iter().map(|id| id as &dyn rusqlite::ToSql).collect();

        let symbol_iter = stmt.query_map(&params[..], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        Ok(symbols)
    }

    /// Find symbols by name with optional language filter
    pub fn find_symbols_by_name(&self, name: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE name = ?1 ORDER BY language, file_path",
            SYMBOL_COLUMNS
        );
        let mut stmt = self.conn.prepare(&query)?;

        let symbol_iter = stmt.query_map(params![name], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!("Found {} symbols named '{}'", symbols.len(), name);
        Ok(symbols)
    }

    /// Get child symbols by parent ID (methods, fields, enum members)
    pub fn get_children_by_parent_id(&self, parent_id: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE parent_id = ?1 ORDER BY start_line",
            SYMBOL_COLUMNS_LIGHTWEIGHT
        );
        let mut stmt = self.conn.prepare(&query)?;
        let symbol_iter =
            stmt.query_map(params![parent_id], |row| self.row_to_symbol_lightweight(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }
        Ok(symbols)
    }

    /// Get symbols for a specific file
    pub fn get_symbols_for_file(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence, content_type
             FROM symbols
             WHERE file_path = ?1
             ORDER BY start_line, start_col",
        )?;

        let symbol_iter = stmt.query_map(params![file_path], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!("Found {} symbols in file '{}'", symbols.len(), file_path);
        Ok(symbols)
    }

    /// Get symbols for a file, skipping expensive columns (code_context, metadata, etc.)
    ///
    /// Use this when the caller doesn't need code bodies or metadata — e.g. structure mode
    /// in get_symbols. Avoids reading large code_context blobs and parsing metadata JSON.
    pub fn get_symbols_for_file_lightweight(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE file_path = ?1 ORDER BY start_line, start_col",
            SYMBOL_COLUMNS_LIGHTWEIGHT
        );
        let mut stmt = self.conn.prepare(&query)?;

        let symbol_iter =
            stmt.query_map(params![file_path], |row| self.row_to_symbol_lightweight(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!(
            "Found {} symbols in file '{}' (lightweight)",
            symbols.len(),
            file_path
        );
        Ok(symbols)
    }
}
