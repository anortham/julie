// Symbol query operations

use super::super::helpers::{SYMBOL_COLUMNS, SYMBOL_COLUMNS_LIGHTWEIGHT};
use super::super::*;
use super::annotations::hydrate_annotations_for_symbols;
use anyhow::Result;
use rusqlite::params;
use std::collections::HashMap;
use tracing::debug;

impl SymbolDatabase {
    pub fn get_symbol_by_id(&self, id: &str) -> Result<Option<Symbol>> {
        let query = format!("SELECT {} FROM symbols WHERE id = ?1", SYMBOL_COLUMNS);
        let mut stmt = self.conn.prepare(&query)?;

        let result = stmt.query_row(params![id], |row| self.row_to_symbol(row));

        match result {
            Ok(mut symbol) => {
                hydrate_annotations_for_symbols(self, std::slice::from_mut(&mut symbol))?;
                Ok(Some(symbol))
            }
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

        let json_ids = serde_json::to_string(ids)
            .map_err(|e| anyhow::anyhow!("Failed to serialize IDs to JSON: {e}"))?;

        let query = format!(
            "SELECT {SYMBOL_COLUMNS} FROM symbols \
             WHERE id IN (SELECT value FROM json_each(?1))"
        );

        let mut stmt = self.conn.prepare(&query)?;
        let symbol_iter = stmt.query_map([&json_ids], |row| self.row_to_symbol(row))?;

        let mut by_id = std::collections::HashMap::new();
        for symbol_result in symbol_iter {
            let symbol = symbol_result?;
            by_id.insert(symbol.id.clone(), symbol);
        }

        let mut symbols: Vec<Symbol> = ids.iter().filter_map(|id| by_id.remove(id)).collect();
        hydrate_annotations_for_symbols(self, &mut symbols)?;
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

        hydrate_annotations_for_symbols(self, &mut symbols)?;
        debug!("Found {} symbols named '{}'", symbols.len(), name);
        Ok(symbols)
    }

    /// Find symbols matching any of the given names in a single batch.
    ///
    /// Groups results by name. Chunks queries to stay within SQLite's parameter limit.
    /// This is O(unique_names) instead of O(total_lookups) — critical for pending
    /// relationship resolution where many relationships share the same callee name.
    pub fn find_symbols_by_names_batch(
        &self,
        names: &[String],
    ) -> Result<HashMap<String, Vec<Symbol>>> {
        if names.is_empty() {
            return Ok(HashMap::new());
        }

        // Deduplicate input names
        let unique_names: Vec<&str> = {
            let mut seen = std::collections::HashSet::new();
            names
                .iter()
                .filter(|n| seen.insert(n.as_str()))
                .map(|n| n.as_str())
                .collect()
        };

        let mut result: HashMap<String, Vec<Symbol>> = HashMap::new();

        // Process in chunks of 500 (well within SQLite's 999 parameter limit)
        const CHUNK_SIZE: usize = 500;
        for chunk in unique_names.chunks(CHUNK_SIZE) {
            let placeholders: String = chunk
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", i + 1))
                .collect::<Vec<_>>()
                .join(",");

            let query = format!(
                "SELECT {} FROM symbols WHERE name IN ({}) ORDER BY name, language, file_path",
                SYMBOL_COLUMNS, placeholders
            );

            let mut stmt = self.conn.prepare(&query)?;
            let params: Vec<&dyn rusqlite::types::ToSql> = chunk
                .iter()
                .map(|n| n as &dyn rusqlite::types::ToSql)
                .collect();

            let symbol_iter = stmt.query_map(&*params, |row| self.row_to_symbol(row))?;
            let mut chunk_symbols = Vec::new();
            for symbol_result in symbol_iter {
                chunk_symbols.push(symbol_result?);
            }

            hydrate_annotations_for_symbols(self, &mut chunk_symbols)?;
            for symbol in chunk_symbols {
                result.entry(symbol.name.clone()).or_default().push(symbol);
            }
        }

        debug!(
            "Batch lookup: {} unique names → {} entries with symbols",
            unique_names.len(),
            result.len()
        );
        Ok(result)
    }

    /// Get child symbols by parent ID (methods, fields, enum members)
    pub fn get_children_by_parent_id(&self, parent_id: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE parent_id = ?1 ORDER BY start_line",
            SYMBOL_COLUMNS_LIGHTWEIGHT
        );
        let mut stmt = self.conn.prepare(&query)?;
        let symbol_iter = stmt.query_map(params![parent_id], |row| {
            self.row_to_symbol_lightweight(row)
        })?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }
        Ok(symbols)
    }

    /// Get symbols for a specific file
    pub fn get_symbols_for_file(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE file_path = ?1 ORDER BY start_line, start_col",
            SYMBOL_COLUMNS
        );
        let mut stmt = self.conn.prepare(&query)?;

        let symbol_iter = stmt.query_map(params![file_path], |row| self.row_to_symbol(row))?;

        let mut symbols = Vec::new();
        for symbol_result in symbol_iter {
            symbols.push(symbol_result?);
        }

        hydrate_annotations_for_symbols(self, &mut symbols)?;
        debug!("Found {} symbols in file '{}'", symbols.len(), file_path);
        Ok(symbols)
    }

    /// Get symbols for a file, skipping expensive columns (code_context, metadata, etc.)
    ///
    /// Return the lowercase symbol names for each of the given file paths in one
    /// batched SQL query.  Only `name` and `file_path` columns are read — much
    /// cheaper than full or lightweight symbol rows.
    ///
    /// Only **definition-kind** symbols are returned (class, struct, function,
    /// etc.).  Import, reference, and other use-site rows are excluded so that
    /// a file importing `Foo` cannot receive the title-exact boost that belongs
    /// only to the file that *defines* `Foo`.
    ///
    /// The kind set here must stay in sync with `DEFINITION_KINDS` in
    /// `src/search/scoring.rs` (kept private there; mirrored here as a SQL
    /// literal to avoid a cross-crate dependency inversion).
    ///
    /// Results are keyed by the **exact** `file_path` string passed in (after
    /// deduplication).  Files that have no indexed symbols simply have no entry
    /// in the returned map.  The query is chunked at 500 paths per statement to
    /// stay comfortably inside SQLite's 999-parameter limit.
    pub fn titles_for_files(&self, file_paths: &[&str]) -> Result<HashMap<String, Vec<String>>> {
        if file_paths.is_empty() {
            return Ok(HashMap::new());
        }

        // Deduplicate while preserving order (order doesn't matter for the map,
        // but determinism helps tests).
        let unique_paths: Vec<&str> = {
            let mut seen = std::collections::HashSet::new();
            file_paths
                .iter()
                .copied()
                .filter(|p| seen.insert(*p))
                .collect()
        };

        let mut result: HashMap<String, Vec<String>> = HashMap::new();

        // Definition kinds — must mirror DEFINITION_KINDS in src/search/scoring.rs.
        const DEFINITION_KIND_FILTER: &str = "'class','struct','interface','trait','enum','function','method',\
             'constructor','module','namespace','type','constant','delegate'";

        const CHUNK_SIZE: usize = 500;
        for chunk in unique_paths.chunks(CHUNK_SIZE) {
            let placeholders: String = (1..=chunk.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",");

            let query = format!(
                "SELECT file_path, name FROM symbols \
                 WHERE file_path IN ({placeholders}) \
                 AND kind IN ({DEFINITION_KIND_FILTER})"
            );

            let mut stmt = self.conn.prepare(&query)?;
            let params: Vec<&dyn rusqlite::types::ToSql> = chunk
                .iter()
                .map(|p| p as &dyn rusqlite::types::ToSql)
                .collect();

            let mut rows = stmt.query(&*params)?;
            while let Some(row) = rows.next()? {
                let file_path: String = row.get(0)?;
                let name: String = row.get(1)?;
                result
                    .entry(file_path)
                    .or_default()
                    .push(name.to_lowercase());
            }
        }

        debug!(
            "titles_for_files: {} paths → {} files with symbols",
            unique_paths.len(),
            result.len()
        );
        Ok(result)
    }

    /// Use this when the caller doesn't need code bodies or metadata — e.g. structure mode
    /// in get_symbols. Avoids reading large code_context blobs and parsing metadata JSON.
    pub fn get_symbols_for_file_lightweight(&self, file_path: &str) -> Result<Vec<Symbol>> {
        let query = format!(
            "SELECT {} FROM symbols WHERE file_path = ?1 ORDER BY start_line, start_col",
            SYMBOL_COLUMNS_LIGHTWEIGHT
        );
        let mut stmt = self.conn.prepare(&query)?;

        let symbol_iter = stmt.query_map(params![file_path], |row| {
            self.row_to_symbol_lightweight(row)
        })?;

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
