// Advanced symbol search and statistics operations

use super::super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::debug;

impl SymbolDatabase {
    pub fn get_symbols_by_semantic_group(&self, semantic_group: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE semantic_group = ?1
        ",
        )?;

        let rows = stmt.query_map([semantic_group], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        Ok(symbols)
    }

    /// Get all symbols from all workspaces (for SearchEngine population)
    pub fn get_all_symbols(&self) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            ORDER BY workspace_id, file_path, start_line
        ",
        )?;

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
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE name = ?1
            ORDER BY file_path, start_line
        ",
        )?;

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

    /// Get symbols by exact name match with workspace filtering
    /// PERFORMANCE: Uses indexed WHERE name = ?1 instead of LIKE for O(log n) lookup
    pub fn get_symbols_by_name_and_workspace(
        &self,
        name: &str,
        workspace_ids: Vec<String>,
    ) -> Result<Vec<Symbol>> {
        if workspace_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause for workspace filtering
        let placeholders = workspace_ids
            .iter()
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(",");

        let query = format!(
            "SELECT id, name, kind, language, file_path, signature,
                    start_line, start_col, end_line, end_col, start_byte, end_byte,
                    doc_comment, visibility, code_context, parent_id,
                    metadata, semantic_group, confidence
             FROM symbols
             WHERE name = ?1 AND workspace_id IN ({})
             ORDER BY workspace_id, file_path, start_line",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Build parameters: name first, then workspace IDs
        let mut params: Vec<&dyn rusqlite::ToSql> = vec![&name as &dyn rusqlite::ToSql];
        let ws_params: Vec<&dyn rusqlite::ToSql> = workspace_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        params.extend(ws_params);

        let rows = stmt.query_map(&params[..], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        debug!(
            "Retrieved {} symbols with exact name '{}' from {} workspace(s)",
            symbols.len(),
            name,
            workspace_ids.len()
        );
        Ok(symbols)
    }

    pub fn get_symbols_without_embeddings(&self, workspace_id: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT s.id, s.name, s.kind, s.language, s.file_path, s.signature,
                   s.start_line, s.start_col, s.end_line, s.end_col, s.start_byte, s.end_byte,
                   s.doc_comment, s.visibility, s.code_context, s.parent_id,
                   s.metadata, s.semantic_group, s.confidence
            FROM symbols s
            LEFT JOIN embeddings e ON s.id = e.symbol_id
            WHERE s.workspace_id = ?1 AND e.symbol_id IS NULL
            ORDER BY s.file_path, s.start_line
        ",
        )?;

        let rows = stmt.query_map([workspace_id], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row_result in rows {
            symbols.push(row_result?);
        }

        Ok(symbols)
    }

    /// Get symbols for a specific workspace (optimized for background tasks)
    pub fn get_symbols_for_workspace(&self, workspace_id: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE workspace_id = ?1
            ORDER BY file_path, start_line
        ",
        )?;

        let rows = stmt.query_map([workspace_id], |row| self.row_to_symbol(row))?;

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
    pub fn get_symbols_batch(
        &self,
        workspace_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, name, kind, language, file_path, signature,
                   start_line, start_col, end_line, end_col, start_byte, end_byte,
                   doc_comment, visibility, code_context, parent_id,
                   metadata, semantic_group, confidence
            FROM symbols
            WHERE workspace_id = ?1
            ORDER BY file_path, start_line
            LIMIT ?2 OFFSET ?3
        ",
        )?;

        let rows = stmt.query_map(
            [workspace_id, &limit.to_string(), &offset.to_string()],
            |row| self.row_to_symbol(row),
        )?;

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

    pub fn get_symbol_count_for_workspace(&self, workspace_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(count)
    }

    /// Get total file count for a workspace (for registry statistics)
    pub fn get_file_count_for_workspace(&self, workspace_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(count)
    }

    /// Get all indexed file paths for a workspace (for staleness detection)
    ///
    /// Returns a vector of relative file paths that are currently indexed in the database
    pub fn get_all_indexed_files(&self, workspace_id: &str) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path FROM files WHERE workspace_id = ?1")?;

        let file_paths: Vec<String> = stmt
            .query_map(params![workspace_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?;

        Ok(file_paths)
    }

    /// Check if workspace has any symbols (quick health check)
    pub fn has_symbols_for_workspace(&self, workspace_id: &str) -> Result<bool> {
        let exists: i64 = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM symbols WHERE workspace_id = ?1 LIMIT 1)",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(exists > 0)
    }

    /// Count total symbols for a workspace (for statistics)
    pub fn count_symbols_for_workspace(&self, workspace_id: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM symbols WHERE workspace_id = ?1",
            params![workspace_id],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    /// Query symbols by name pattern (LIKE search) with optional filters
    /// Uses idx_symbols_name, idx_symbols_language, idx_symbols_workspace for fast lookup
    pub fn query_symbols_by_name_pattern(
        &self,
        pattern: &str,
        language: Option<&str>,
        workspace_ids: &[String],
    ) -> Result<Vec<Symbol>> {
        let pattern_like = format!("%{}%", pattern);

        let query = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            if let Some(_lang) = language {
                format!(
                    "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                            end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                            parent_id, metadata, semantic_group, confidence
                     FROM symbols
                     WHERE (name LIKE ?1 OR code_context LIKE ?1) AND language = ?2 AND workspace_id IN ({})
                     ORDER BY name, file_path
                     LIMIT 1000",
                    workspace_placeholders
                )
            } else {
                format!(
                    "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                            end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                            parent_id, metadata, semantic_group, confidence
                     FROM symbols
                     WHERE (name LIKE ?1 OR code_context LIKE ?1) AND workspace_id IN ({})
                     ORDER BY name, file_path
                     LIMIT 1000",
                    workspace_placeholders
                )
            }
        } else if language.is_some() {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE (name LIKE ?1 OR code_context LIKE ?1) AND language = ?2
             ORDER BY name, file_path
             LIMIT 1000"
                .to_string()
        } else {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE (name LIKE ?1 OR code_context LIKE ?1)
             ORDER BY name, file_path
             LIMIT 1000"
                .to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;

        // Build params dynamically
        let symbols = if let Some(lang) = language {
            let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&pattern_like, &lang];
            for ws_id in workspace_ids {
                params_vec.push(ws_id);
            }
            let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            result
        } else {
            let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&pattern_like];
            for ws_id in workspace_ids {
                params_vec.push(ws_id);
            }
            let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;
            let mut result = Vec::new();
            for row in rows {
                result.push(row?);
            }
            result
        };

        Ok(symbols)
    }

    /// Query symbols by kind with workspace filtering
    /// Uses idx_symbols_kind, idx_symbols_workspace for fast lookup
    pub fn query_symbols_by_kind(
        &self,
        kind: &SymbolKind,
        workspace_ids: &[String],
    ) -> Result<Vec<Symbol>> {
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

        let query = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                        end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                        parent_id, metadata, semantic_group, confidence
                 FROM symbols
                 WHERE kind = ?1 AND workspace_id IN ({})
                 ORDER BY file_path, start_line",
                workspace_placeholders
            )
        } else {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE kind = ?1
             ORDER BY file_path, start_line"
                .to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;

        let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&kind_str];
        for ws_id in workspace_ids {
            params_vec.push(ws_id);
        }

        let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for row in rows {
            symbols.push(row?);
        }

        Ok(symbols)
    }

    /// Query symbols by language with workspace filtering
    /// Uses idx_symbols_language, idx_symbols_workspace for fast lookup
    pub fn query_symbols_by_language(
        &self,
        language: &str,
        workspace_ids: &[String],
    ) -> Result<Vec<Symbol>> {
        let query = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                        end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                        parent_id, metadata, semantic_group, confidence
                 FROM symbols
                 WHERE language = ?1 AND workspace_id IN ({})
                 ORDER BY file_path, start_line",
                workspace_placeholders
            )
        } else {
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE language = ?1
             ORDER BY file_path, start_line"
                .to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;

        let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&language];
        for ws_id in workspace_ids {
            params_vec.push(ws_id);
        }

        let rows = stmt.query_map(params_vec.as_slice(), |row| self.row_to_symbol(row))?;

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
        workspace_ids: &[String],
    ) -> Result<(
        std::collections::HashMap<String, usize>,
        std::collections::HashMap<String, usize>,
    )> {
        use std::collections::HashMap;

        let mut by_kind = HashMap::new();
        let mut by_language = HashMap::new();

        // Count by kind
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let kind_query = format!(
                "SELECT kind, COUNT(*) as count FROM symbols WHERE workspace_id IN ({}) GROUP BY kind",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&kind_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (kind, count) = row?;
                by_kind.insert(kind, count);
            }
        } else {
            let kind_query = "SELECT kind, COUNT(*) as count FROM symbols GROUP BY kind";
            let mut stmt = self.conn.prepare(kind_query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (kind, count) = row?;
                by_kind.insert(kind, count);
            }
        }

        // Count by language
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let lang_query = format!(
                "SELECT language, COUNT(*) as count FROM symbols WHERE workspace_id IN ({}) GROUP BY language",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&lang_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (language, count) = row?;
                by_language.insert(language, count);
            }
        } else {
            let lang_query = "SELECT language, COUNT(*) as count FROM symbols GROUP BY language";
            let mut stmt = self.conn.prepare(lang_query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (language, count) = row?;
                by_language.insert(language, count);
            }
        }

        Ok((by_kind, by_language))
    }
    pub fn get_file_statistics(
        &self,
        workspace_ids: &[String],
    ) -> Result<std::collections::HashMap<String, usize>> {
        use std::collections::HashMap;

        let mut by_file = HashMap::new();

        // Count symbols per file using SQL GROUP BY
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let file_query = format!(
                "SELECT file_path, COUNT(*) as count FROM symbols WHERE workspace_id IN ({}) GROUP BY file_path",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&file_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (file_path, count) = row?;
                by_file.insert(file_path, count);
            }
        } else {
            let file_query = "SELECT file_path, COUNT(*) as count FROM symbols GROUP BY file_path";
            let mut stmt = self.conn.prepare(file_query)?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (file_path, count) = row?;
                by_file.insert(file_path, count);
            }
        }

        Ok(by_file)
    }

    /// Get total symbol count using SQL COUNT (O(1) database operation)
    pub fn get_total_symbol_count(&self, workspace_ids: &[String]) -> Result<usize> {
        let count: i64 = if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let count_query = format!(
                "SELECT COUNT(*) FROM symbols WHERE workspace_id IN ({})",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&count_query)?;
            let params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            stmt.query_row(params.as_slice(), |row| row.get(0))?
        } else {
            let count_query = "SELECT COUNT(*) FROM symbols";
            let mut stmt = self.conn.prepare(count_query)?;
            stmt.query_row([], |row| row.get(0))?
        };

        Ok(count as usize)
    }

    /// Get file-level relationship statistics using SQL (for hotspot analysis)
    ///
    /// Returns: HashMap<file_path, relationship_count> counting relationships where symbols from this file participate
    pub fn get_most_referenced_symbols(
        &self,
        workspace_ids: &[String],
        limit: usize,
    ) -> Result<Vec<(String, usize)>> {
        let mut results = Vec::new();

        // SQL GROUP BY aggregation - counts incoming references per symbol
        if !workspace_ids.is_empty() {
            let workspace_placeholders = workspace_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(",");
            let query = format!(
                "SELECT to_symbol_id, COUNT(*) as ref_count \
                 FROM relationships \
                 WHERE workspace_id IN ({}) \
                 GROUP BY to_symbol_id \
                 ORDER BY ref_count DESC \
                 LIMIT ?",
                workspace_placeholders
            );

            let mut stmt = self.conn.prepare(&query)?;
            let mut params: Vec<&dyn rusqlite::ToSql> = workspace_ids
                .iter()
                .map(|id| id as &dyn rusqlite::ToSql)
                .collect();
            params.push(&limit);

            let rows = stmt.query_map(params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?;

            for row in rows {
                let (symbol_id, count) = row?;
                results.push((symbol_id, count));
            }
        } else {
            // No workspace filter - count all references
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
        }

        Ok(results)
    }

}
