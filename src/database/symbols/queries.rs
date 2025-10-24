// Symbol query operations

use super::super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::debug;

impl SymbolDatabase {
    pub fn get_symbol_by_id(&self, id: &str) -> Result<Option<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols WHERE id = ?1",
        )?;

        let result = stmt.query_row(params![id], |row| self.row_to_symbol(row));

        match result {
            Ok(symbol) => Ok(Some(symbol)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(anyhow!("Database error: {}", e)),
        }
    }

    /// Get multiple symbols by their IDs in one batched query (for semantic search results)
    pub fn get_symbols_by_ids(&self, ids: &[String]) -> Result<Vec<Symbol>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause for batch fetch
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{}", i)).collect();
        let query = format!(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols WHERE id IN ({})",
            placeholders.join(", ")
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
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
             FROM symbols
             WHERE name = ?1
             ORDER BY language, file_path",
        )?;

        let symbol_iter = stmt.query_map(params![name], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!("Found {} symbols named '{}'", symbols.len(), name);
        Ok(symbols)
    }

    /// 🔒 FTS5 Query Sanitization - Escape special characters that cause syntax errors
    ///
    /// FTS5 has several special characters that trigger specific behaviors:
    /// - `#` - Column specifier (e.g., `name:#term`)
    /// - `@` - Auxiliary function calls
    /// - `^` - Initial token match
    /// - `:` - Can be interpreted as column separator
    /// - `[` `]` - Special meaning in some contexts
    ///
    /// Strategy:
    /// 1. If query is already quoted → pass through as-is (user knows what they want)
    /// 2. If query contains intentional operators (AND, OR, NOT, *, ") → pass through
    /// 3. If query contains special characters → quote the entire query as a phrase
    /// 4. Multi-word queries → use OR for forgiving search
    /// 5. Otherwise → pass through as-is (simple term search)
    ///
    /// Works with unicode61 tokenizer configured with separators "_::->.":
    /// - "user_service" → tokenized as ["user", "service"] at index time
    /// - "std::vector" → tokenized as ["std", "vector"] at index time
    /// - Queries naturally match individual tokens
    pub(crate) fn sanitize_fts5_query(query: &str) -> String {
        let trimmed = query.trim();

        // Empty queries pass through (will return no results anyway)
        if trimmed.is_empty() {
            return trimmed.to_string();
        }

        // Already quoted - user explicitly wants phrase search
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return trimmed.to_string();
        }

        // 🔥 FIX: Remove regex escape backslashes early
        // Users coming from grep/ripgrep might use \. \d \w etc.
        // These have no meaning in FTS5 and cause syntax errors
        // Strip them out early so downstream logic doesn't have to handle them
        let trimmed = trimmed.replace('\\', "");

        // Contains explicit FTS5 operators - pass through (user knows FTS5 syntax)
        if trimmed.contains(" AND ") || trimmed.contains(" OR ") || trimmed.contains(" NOT ") {
            return trimmed.to_string();
        }

        // 🔥 FIX: Handle regex-like patterns that FTS5 can't parse
        // Patterns like "InputFile.*" or "end$" look like regex but cause FTS5 syntax errors
        // These need to be quoted as literal phrases
        let has_regex_metachar = trimmed.contains('$') || trimmed.contains('^');
        let has_dot_star = trimmed.contains(".*");

        if has_regex_metachar || has_dot_star {
            // Quote as phrase to prevent FTS5 from interpreting these as operators
            let escaped = trimmed.replace('"', "\"\"");
            return format!("\"{}\"", escaped);
        }

        // Contains intentional wildcards - pass through
        // (But only if not combined with problematic chars like . or $, handled above)
        if trimmed.contains('*') {
            return trimmed.to_string();
        }

        // 🔥 FIX: Handle : (colon) specially - it's a tokenizer separator BUT also FTS5 column syntax
        // FTS5 treats : as column specification syntax (e.g., "name:term")
        // So "foo:bar" is interpreted as "column foo, term bar" which causes "no such column: foo" error
        // Split on : and convert to OR query to work with our separator tokenization
        // Handle both :: (scope resolution) and : (single colon)
        if trimmed.contains(':') {
            let parts: Vec<&str> = trimmed.split(':').filter(|s| !s.is_empty()).collect();
            if parts.len() > 1 {
                return parts.join(" OR ");
            }
        }

        // 🔥 FIX: Handle . (dot) specially - it's a tokenizer separator BUT also FTS5 column syntax
        // FTS5 treats . as column specification (e.g., "table.column") BEFORE tokenization
        // So "CurrentUserService.ApplicationUser" causes "syntax error near '.'"
        // Solution: Split on . and OR the parts to match tokenized content
        // Example: "System.Collections.Generic" → "System OR Collections OR Generic"
        if trimmed.contains('.') && !trimmed.chars().all(|c| c.is_ascii_digit() || c == '.') {
            // Don't split numbers like "3.14" - only split identifier-like strings
            let parts: Vec<&str> = trimmed.split('.').filter(|s| !s.is_empty()).collect();
            if parts.len() > 1 {
                return parts.join(" OR ");
            }
        }

        // FTS5 special characters that need escaping
        // Note: Removed separators (_ - > .) since they're now tokenizer delimiters
        // : is a separator BUT also FTS5 column syntax, handled specially above
        // . is a separator BUT also FTS5 column syntax, handled specially above
        // + is not officially documented as special, but causes "syntax error near +" in practice
        // ! is used for NOT operator, ( ) for grouping - all need escaping when literal
        // = causes "syntax error near =" when used in queries
        // | causes "syntax error near |" (regex alternation, not FTS5)
        const SPECIAL_CHARS: &[char] = &[
            '#', '@', '^', '[', ']', '+', '/', '\\', '!', '(', ')', '=', '|',
        ];

        // Check if query contains any special characters
        let has_special = trimmed.chars().any(|c| SPECIAL_CHARS.contains(&c));

        if has_special {
            // Quote the entire query to treat it as a literal phrase
            // Use double quotes and escape any internal double quotes
            // (Backslashes already stripped earlier in this function)
            let escaped = trimmed.replace('"', "\"\""); // FTS5 uses doubled quotes for escaping
            format!("\"{}\"", escaped)
        } else {
            // 🔥 FIX: Multi-word queries should use OR, not implicit AND
            // "refresh workspace embedding" → "refresh OR workspace OR embedding"
            // This makes search more forgiving and user-friendly
            let words: Vec<&str> = trimmed.split_whitespace().collect();
            if words.len() > 1 {
                // Multi-word query without operators - use OR for flexibility
                words.join(" OR ")
            } else {
                // Single word - pass through as-is
                trimmed.to_string()
            }
        }
    }

    /// 🔥 CASCADE FTS5: Find symbols using full-text search with BM25 ranking
    /// Replaces slow LIKE queries with fast FTS5 MATCH queries
    /// Column weights: name (10x), signature (5x), doc_comment (2x), code_context (1x)
    /// Note: workspace_ids kept for API, DB file is already workspace-specific
    pub fn find_symbols_by_pattern(&self, pattern: &str) -> Result<Vec<Symbol>> {
        // 🔒 CRITICAL FIX: Sanitize query to prevent FTS5 syntax errors from special characters
        let sanitized_pattern = Self::sanitize_fts5_query(pattern);
        debug!(
            "🔍 FTS5 query sanitization: '{}' -> '{}'",
            pattern, sanitized_pattern
        );

        // 🔥 FTS5 MATCH with BM25 ranking - no workspace filter needed
        // Prioritize exact name matches with 10x weight, then signature (5x), doc_comment (2x), code_context (1x)
        let query = "SELECT s.id, s.name, s.kind, s.language, s.file_path, s.signature, s.start_line, s.start_col,
                           s.end_line, s.end_col, s.start_byte, s.end_byte, s.doc_comment, s.visibility, s.code_context,
                           s.parent_id, s.metadata, s.semantic_group, s.confidence
                     FROM symbols s
                     INNER JOIN symbols_fts fts ON s.rowid = fts.rowid
                     WHERE symbols_fts MATCH ?1
                     ORDER BY bm25(symbols_fts, 10.0, 5.0, 2.0, 1.0)";

        let mut stmt = self.conn.prepare(query)?;
        let symbol_iter = stmt.query_map([&sanitized_pattern], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        debug!(
            "🔍 FTS5: Found {} symbols matching '{}' (BM25 ranked)",
            symbols.len(),
            pattern
        );
        Ok(symbols)
    }

    /// Get symbols for a specific file
    pub fn get_symbols_for_file(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, language, file_path, signature, start_line, start_col,
                    end_line, end_col, start_byte, end_byte, doc_comment, visibility, code_context,
                    parent_id, metadata, semantic_group, confidence
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
}
