// Advanced symbol search and statistics operations

use super::super::helpers::SYMBOL_COLUMNS;
use super::super::*;
use anyhow::Result;
use tracing::debug;

impl SymbolDatabase {
    pub fn get_symbols_by_semantic_group(&self, semantic_group: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE semantic_group = ?1",
            SYMBOL_COLUMNS
        );
        let mut stmt = self.conn.prepare(&query)?;

        let rows = stmt.query_map([semantic_group], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        Ok(symbols)
    }

    /// Get all symbols from all workspaces (for SearchEngine population)
    pub fn get_all_symbols(&self) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols ORDER BY file_path, start_line",
            SYMBOL_COLUMNS
        );
        let mut stmt = self.conn.prepare(&query)?;

        let rows = stmt.query_map([], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols from database for SearchEngine",
            symbols.len()
        );
        Ok(symbols)
    }

    /// Get all symbols matching an exact name (indexed lookup)
    /// Used to replace in-memory Vec<Symbol> fallbacks with persistent SQLite queries
    pub fn get_symbols_by_name(&self, name: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE name = ?1 ORDER BY file_path, start_line",
            SYMBOL_COLUMNS
        );
        let mut stmt = self.conn.prepare(&query)?;

        let rows = stmt.query_map([name], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols with name '{}' from database",
            symbols.len(),
            name
        );
        Ok(symbols)
    }

    /// Get symbols by exact name match
    /// PERFORMANCE: Uses indexed WHERE name = ?1 instead of LIKE for O(log n) lookup
    pub fn get_symbols_by_name_and_workspace(&self, name: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature,
                    start_line, start_col, end_line, end_col, start_byte, end_byte,
                    doc_comment, visibility, code_context, parent_id,
                    metadata, semantic_group, confidence
             FROM symbols
             WHERE name = ?1
             ORDER BY file_path, start_line",
        )?;

        let rows = stmt.query_map([name], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols with exact name '{}'",
            symbols.len(),
            name
        );
        Ok(symbols)
    }

    pub fn get_symbols_without_embeddings(&self) -> Result<Vec<Symbol>> {
        // Need to prefix columns with "s." for the JOIN query
        let columns_with_prefix = SYMBOL_COLUMNS
            .split(", ")
            .map(|col| format!("s.{}", col))
            .collect::<Vec<_>>()
            .join(", ");

        // BUG #3 FIX: Filter out un-embeddable symbols
        // - Markdown headings without doc comments (build_embedding_text returns empty)
        // - Memory JSON symbols except "description" (build_embedding_text returns empty)
        let query = format!(
            "SELECT {} FROM symbols s
             LEFT JOIN embeddings e ON s.id = e.symbol_id
             WHERE e.symbol_id IS NULL
               AND NOT (s.language = 'markdown' AND (s.doc_comment IS NULL OR s.doc_comment = ''))
               AND NOT (s.file_path LIKE '.memories/%' AND s.name != 'description')
             ORDER BY s.file_path, s.start_line",
            columns_with_prefix
        );
        let mut stmt = self.conn.prepare(&query)?;

        let rows = stmt.query_map([], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        Ok(symbols)
    }

    /// Get symbols for a specific workspace (optimized for background tasks)
    /// Note: workspace_id parameter kept for logging, but DB file is already workspace-specific
    pub fn get_symbols_for_workspace(&self, workspace_id: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols ORDER BY file_path, start_line",
            SYMBOL_COLUMNS
        );
        let mut stmt = self.conn.prepare(&query)?;

        let rows = stmt.query_map([], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols for workspace '{}' from database",
            symbols.len(),
            workspace_id
        );
        Ok(symbols)
    }

    /// Get file hashes for a specific workspace for incremental update detection
    /// Note: workspace_id kept for logging, DB file is already workspace-specific
    pub fn get_symbols_batch(
        &self,
        workspace_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols ORDER BY file_path, start_line LIMIT ?1 OFFSET ?2",
            SYMBOL_COLUMNS
        );
        let mut stmt = self.conn.prepare(&query)?;

        let rows = stmt.query_map([&limit.to_string(), &offset.to_string()], |row| {
            self.row_to_symbol(row)
        })?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved batch of {} symbols (offset: {}, limit: {}) for workspace '{}'",
            symbols.len(),
            offset,
            limit,
            workspace_id
        );
        Ok(symbols)
    }

    pub fn get_symbol_count_for_workspace(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        Ok(count)
    }

    /// Get total file count for a workspace (for registry statistics)
    pub fn get_file_count_for_workspace(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;

        Ok(count)
    }

    /// Get all indexed file paths for a workspace (for staleness detection)
    ///
    /// Returns a vector of relative file paths that are currently indexed in the database
    /// Note: workspace_id kept for API, DB file is already workspace-specific
    pub fn get_all_indexed_files(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM files")?;

        let file_paths: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;

        Ok(file_paths)
    }

    /// Check if workspace has any symbols (quick health check)
    pub fn has_symbols_for_workspace(&self) -> Result<bool> {
        let exists: i64 =
            self.conn
                .query_row("SELECT EXISTS(SELECT 1 FROM symbols LIMIT 1)", [], |row| {
                    row.get(0)
                })?;

        Ok(exists > 0)
    }

    /// Count total symbols for a workspace (for statistics)
    pub fn count_symbols_for_workspace(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        Ok(count as usize)
    }

    /// Query symbols by name pattern (LIKE search) with optional filters
    /// Uses idx_symbols_name, idx_symbols_language for fast lookup
    pub fn query_symbols_by_name_pattern(
        &self,
        pattern: &str,
        language: Option<&str>,
    ) -> Result<Vec<Symbol>> {
        let pattern_like = format!("%{}%", pattern);

        let mut symbols = Vec::new();

        if let Some(lang) = language {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                        end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                        parent_id, metadata, semantic_group, confidence
                 FROM symbols
                 WHERE (name LIKE ?1 OR code_context LIKE ?1) AND language = ?2
                 ORDER BY name, file_path
                 LIMIT 1000"
            )?;
            let rows =
                stmt.query_map([&pattern_like as &str, lang], |row| self.row_to_symbol(row))?;
            for row in rows {
                symbols.push(row?);
            }
        } else {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                        end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                        parent_id, metadata, semantic_group, confidence
                 FROM symbols
                 WHERE (name LIKE ?1 OR code_context LIKE ?1)
                 ORDER BY name, file_path
                 LIMIT 1000"
            )?;
            let rows = stmt.query_map([&pattern_like], |row| self.row_to_symbol(row))?;
            for row in rows {
                symbols.push(row?);
            }
        }

        Ok(symbols)
    }

    /// Query symbols by kind
    /// Uses idx_symbols_kind for fast lookup
    pub fn query_symbols_by_kind(&self, kind: &SymbolKind) -> Result<Vec<Symbol>> {
        let kind_str = match kind {
            SymbolKind::Function => "function",
            SymbolKind::Method => "method",
            SymbolKind::Class => "class",
            SymbolKind::Interface => "interface",
            SymbolKind::Enum => "enum",
            SymbolKind::Struct => "struct",
            SymbolKind::Variable => "variable",
            SymbolKind::Constant => "constant",
            SymbolKind::Property => "property",
            SymbolKind::Module => "module",
            SymbolKind::Namespace => "namespace",
            SymbolKind::Type => "type",
            SymbolKind::Trait => "trait",
            SymbolKind::Union => "union",
            SymbolKind::Field => "field",
            SymbolKind::Constructor => "constructor",
            SymbolKind::Destructor => "destructor",
            SymbolKind::Operator => "operator",
            SymbolKind::Import => "import",
            SymbolKind::Export => "export",
            SymbolKind::Event => "event",
            SymbolKind::Delegate => "delegate",
            SymbolKind::EnumMember => "enum_member",
        };

        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE kind = ?1
             ORDER BY file_path, start_line",
        )?;

        let rows = stmt.query_map([&kind_str], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row in rows {
            symbols.push(row?);
        }

        Ok(symbols)
    }

    /// Query symbols by language
    /// Uses idx_symbols_language for fast lookup
    pub fn query_symbols_by_language(&self, language: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE language = ?1
             ORDER BY file_path, start_line",
        )?;

        let rows = stmt.query_map([language], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row in rows {
            symbols.push(row?);
        }

        Ok(symbols)
    }

    /// Get aggregate symbol statistics (fast COUNT queries with GROUP BY)
    /// Returns counts by kind and by language
    pub fn get_symbol_statistics(
        &self,
    ) -> Result<(
        std::collections::HashMap<String, usize>,
        std::collections::HashMap<String, usize>,
    )> {
        use std::collections::HashMap;

        let mut by_kind = HashMap::new();
        let mut by_language = HashMap::new();

        // Count by kind
        let kind_query = "SELECT kind, COUNT(*) as count FROM symbols GROUP BY kind";
        let mut stmt = self.conn.prepare(kind_query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;

        for row in rows {
            let (kind, count) = row?;
            by_kind.insert(kind, count);
        }

        // Count by language
        let lang_query = "SELECT language, COUNT(*) as count FROM symbols GROUP BY language";
        let mut stmt = self.conn.prepare(lang_query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;

        for row in rows {
            let (language, count) = row?;
            by_language.insert(language, count);
        }

        Ok((by_kind, by_language))
    }
    /// Get file-level symbol counts (GROUP BY file_path)
    pub fn get_file_statistics(&self) -> Result<std::collections::HashMap<String, usize>> {
        use std::collections::HashMap;

        let mut by_file = HashMap::new();

        // Count symbols per file using SQL GROUP BY
        let file_query = "SELECT file_path, COUNT(*) as count FROM symbols GROUP BY file_path";
        let mut stmt = self.conn.prepare(file_query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;

        for row in rows {
            let (file_path, count) = row?;
            by_file.insert(file_path, count);
        }

        Ok(by_file)
    }

    /// Get total symbol count using SQL COUNT (O(1) database operation)
    pub fn get_total_symbol_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;

        Ok(count as usize)
    }

    /// Get most referenced symbols (GROUP BY aggregation on relationships)
    pub fn get_most_referenced_symbols(&self, limit: usize) -> Result<Vec<(String, usize)>> {
        let mut results = Vec::new();

        // SQL GROUP BY aggregation - counts incoming references per symbol
        let query = "SELECT to_symbol_id, COUNT(*) as ref_count \
                     FROM relationships \
                     GROUP BY to_symbol_id \
                     ORDER BY ref_count DESC \
                     LIMIT ?";

        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;

        for row in rows {
            let (symbol_id, count) = row?;
            results.push((symbol_id, count));
        }

        Ok(results)
    }
}
