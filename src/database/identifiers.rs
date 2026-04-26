// Identifier query operations - unlocking the identifiers table for fast_refs
//
// The identifiers table stores every usage site extracted by all 31 language extractors.
// Each row represents a place where a symbol name appears in code (calls, type usages,
// member access, imports, variable references). This module provides read access to
// that data, which fast_refs uses to find references beyond what the relationships
// table captures.

use super::*;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::debug;

/// Lightweight identifier reference — just the fields fast_refs needs.
/// Avoids pulling the full Identifier struct (which has byte offsets, end positions, etc.)
#[derive(Debug, Clone)]
pub struct IdentifierRef {
    pub name: String,
    pub kind: String, // "call", "variable_ref", "type_usage", "member_access", "import"
    pub file_path: String,
    pub start_line: u32,
    pub containing_symbol_id: Option<String>,
    /// ID of the symbol this identifier resolves to, if resolution ran.
    /// Used by blast_radius to prefer resolved target matches over name-only
    /// fallbacks when collecting likely tests.
    pub target_symbol_id: Option<String>,
    pub confidence: f32,
}

/// Column list for IdentifierRef queries
const IDENTIFIER_REF_COLUMNS: &str =
    "name, kind, file_path, start_line, containing_symbol_id, target_symbol_id, confidence";

/// Escape SQL LIKE wildcard characters so they match literally.
/// `_` (any single char) and `%` (any sequence) are escaped with `\`.
/// The backslash itself is also escaped.
fn escape_sql_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            _ => out.push(ch),
        }
    }
    out
}

/// Build WHERE clause that matches both exact names AND qualified names (e.g. Type::method).
/// Identifiers are stored as qualified calls like "CodeTokenizer::new" but agents search
/// for just "CodeTokenizer". This generates:
///   WHERE name IN (?, ?) OR name LIKE ? || '::%' OR name LIKE ? || '.%' ...
fn build_name_match_clause(names: &[String]) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    let mut idx = 1;

    // Exact match: name IN (?, ?, ...)
    let exact_placeholders: Vec<String> = names
        .iter()
        .map(|name| {
            let p = format!("?{}", idx);
            params.push(Box::new(name.clone()));
            idx += 1;
            p
        })
        .collect();
    let exact_clause = format!("name IN ({})", exact_placeholders.join(", "));

    // Prefix match: name LIKE 'Symbol::%' OR name LIKE 'Symbol.%'
    // Catches qualified calls like CodeTokenizer::new, self.method, etc.
    let mut prefix_conditions = Vec::new();
    for name in names {
        // Rust-style :: qualifier
        prefix_conditions.push(format!("name LIKE ?{} ESCAPE '\\'", idx));
        params.push(Box::new(format!("{}::%", escape_sql_like(name))));
        idx += 1;
        // Dot-style qualifier (most other languages)
        prefix_conditions.push(format!("name LIKE ?{} ESCAPE '\\'", idx));
        params.push(Box::new(format!("{}.%", escape_sql_like(name))));
        idx += 1;
    }

    let clause = if prefix_conditions.is_empty() {
        exact_clause
    } else {
        format!("({} OR {})", exact_clause, prefix_conditions.join(" OR "))
    };

    (clause, params)
}

impl SymbolDatabase {
    /// Convert a database row to an IdentifierRef
    fn row_to_identifier_ref(&self, row: &Row) -> rusqlite::Result<IdentifierRef> {
        Ok(IdentifierRef {
            name: row.get("name")?,
            kind: row.get("kind")?,
            file_path: row.get("file_path")?,
            start_line: row.get("start_line")?,
            containing_symbol_id: row.get("containing_symbol_id")?,
            target_symbol_id: row.get("target_symbol_id")?,
            confidence: row.get("confidence")?,
        })
    }

    /// Find all identifiers matching any of the given names or their qualified forms.
    /// Matches both exact names ("CodeTokenizer") and qualified calls ("CodeTokenizer::new").
    /// Uses idx_identifiers_name index for exact matches; LIKE for prefix matches.
    /// Chunked in batches of 166 names (each name uses 3 bind params: exact + 2 prefix patterns).
    pub fn get_identifiers_by_names(&self, names: &[String]) -> Result<Vec<IdentifierRef>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        // Each name produces 3 bind params (1 exact + 2 prefix LIKE patterns).
        // Chunk at 166 names to stay comfortably under the 999-param limit.
        const MAX_NAMES_PER_CHUNK: usize = 166;
        let mut results = Vec::new();

        for chunk in names.chunks(MAX_NAMES_PER_CHUNK) {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let (where_clause, params) = build_name_match_clause(&chunk_vec);
            let query = format!(
                "SELECT {} FROM identifiers WHERE {}",
                IDENTIFIER_REF_COLUMNS, where_clause
            );

            let mut stmt = self.conn.prepare(&query)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = params
                .iter()
                .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
                .collect();

            let rows = stmt.query_map(&param_refs[..], |row| self.row_to_identifier_ref(row))?;
            for row in rows {
                results.push(row?);
            }
        }

        debug!(
            "Found {} identifiers for {} name variants (with prefix matching)",
            results.len(),
            names.len()
        );
        Ok(results)
    }

    /// Get all call identifiers grouped by containing_symbol_id.
    ///
    /// Returns a HashMap mapping symbol_id → Vec<callee_name>.
    /// Used by analysis passes that need batched call names.
    pub fn get_call_identifiers_grouped(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT containing_symbol_id, name FROM identifiers WHERE kind = 'call' AND containing_symbol_id IS NOT NULL"
        )?;

        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (symbol_id, callee_name) = row?;
            grouped.entry(symbol_id).or_default().push(callee_name);
        }

        // Deduplicate within each symbol (a function calling process() 5 times
        // should only list it once in embedding enrichment text).
        for callees in grouped.values_mut() {
            callees.sort();
            callees.dedup();
        }

        debug!(
            "Loaded {} call identifiers across {} symbols",
            grouped.values().map(|v| v.len()).sum::<usize>(),
            grouped.len()
        );

        Ok(grouped)
    }

    /// Get all member_access identifiers grouped by containing_symbol_id.
    ///
    /// Returns a HashMap mapping symbol_id -> Vec<field_name>.
    /// Used by embedding enrichment to capture domain vocabulary from field accesses.
    pub fn get_member_access_identifiers_grouped(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT containing_symbol_id, name FROM identifiers WHERE kind = 'member_access' AND containing_symbol_id IS NOT NULL"
        )?;

        let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (symbol_id, field_name) = row?;
            grouped.entry(symbol_id).or_default().push(field_name);
        }

        // Deduplicate within each symbol
        for names in grouped.values_mut() {
            names.sort();
            names.dedup();
        }

        debug!(
            "Loaded {} member_access identifiers across {} symbols",
            grouped.values().map(|v| v.len()).sum::<usize>(),
            grouped.len()
        );

        Ok(grouped)
    }

    /// Find identifiers matching any of the given names or qualified forms, filtered by kind.
    /// Used for reference_kind filtering in fast_refs.
    /// Chunked in batches of 165 names (3 params per name + 1 for kind = 496 params per chunk).
    pub fn get_identifiers_by_names_and_kind(
        &self,
        names: &[String],
        kind: &str,
    ) -> Result<Vec<IdentifierRef>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        // Each name uses 3 params; one extra slot for the kind filter.
        // 165 names × 3 + 1 = 496 params per chunk, safely under the 999-param limit.
        const MAX_NAMES_PER_CHUNK: usize = 165;
        let mut results = Vec::new();

        for chunk in names.chunks(MAX_NAMES_PER_CHUNK) {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let (where_clause, mut params) = build_name_match_clause(&chunk_vec);
            let kind_idx = params.len() + 1;
            params.push(Box::new(kind.to_string()));

            let query = format!(
                "SELECT {} FROM identifiers WHERE {} AND kind = ?{}",
                IDENTIFIER_REF_COLUMNS, where_clause, kind_idx
            );

            let mut stmt = self.conn.prepare(&query)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = params
                .iter()
                .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
                .collect();

            let rows = stmt.query_map(&param_refs[..], |row| self.row_to_identifier_ref(row))?;
            for row in rows {
                results.push(row?);
            }
        }

        debug!(
            "Found {} identifiers for {} name variants with kind='{}' (with prefix matching)",
            results.len(),
            names.len(),
            kind
        );
        Ok(results)
    }

    /// Find identifiers by name and kind while excluding known containers in SQL
    /// when the exclusion set is small enough for SQLite bind limits.
    pub fn get_identifiers_by_names_kinds_excluding_containers(
        &self,
        names: &[String],
        kinds: &[&str],
        excluded_container_ids: &HashSet<String>,
    ) -> Result<Vec<IdentifierRef>> {
        if names.is_empty() || kinds.is_empty() {
            return Ok(Vec::new());
        }

        const MAX_BIND_PARAMS: usize = 900;
        const MAX_SQL_EXCLUSIONS: usize = 300;
        let exclude_in_sql = excluded_container_ids.len() <= MAX_SQL_EXCLUSIONS;
        let exclusion_bind_count = if exclude_in_sql {
            excluded_container_ids.len()
        } else {
            0
        };
        let fixed_bind_count = kinds.len() + exclusion_bind_count;
        let max_names_per_chunk = ((MAX_BIND_PARAMS.saturating_sub(fixed_bind_count)) / 3).max(1);
        let mut results = Vec::new();

        for chunk in names.chunks(max_names_per_chunk) {
            let chunk_vec: Vec<String> = chunk.to_vec();
            let (where_clause, mut params) = build_name_match_clause(&chunk_vec);

            let kind_placeholders = kinds
                .iter()
                .map(|kind| {
                    let idx = params.len() + 1;
                    params.push(Box::new((*kind).to_string()) as Box<dyn rusqlite::ToSql>);
                    format!("?{}", idx)
                })
                .collect::<Vec<_>>()
                .join(", ");

            let exclusion_clause = if exclude_in_sql && !excluded_container_ids.is_empty() {
                let placeholders = excluded_container_ids
                    .iter()
                    .map(|container_id| {
                        let idx = params.len() + 1;
                        params.push(Box::new(container_id.clone()) as Box<dyn rusqlite::ToSql>);
                        format!("?{}", idx)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(" AND containing_symbol_id NOT IN ({})", placeholders)
            } else {
                String::new()
            };

            let query = format!(
                "SELECT {} FROM identifiers \
                 WHERE {} \
                 AND kind IN ({}) \
                 AND containing_symbol_id IS NOT NULL{}",
                IDENTIFIER_REF_COLUMNS, where_clause, kind_placeholders, exclusion_clause
            );

            let mut stmt = self.conn.prepare(&query)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = params
                .iter()
                .map(|p| p.as_ref() as &dyn rusqlite::ToSql)
                .collect();

            let rows = stmt.query_map(&param_refs[..], |row| self.row_to_identifier_ref(row))?;
            for row in rows {
                let identifier = row?;
                if !excluded_container_ids.contains(
                    identifier
                        .containing_symbol_id
                        .as_deref()
                        .unwrap_or_default(),
                ) {
                    results.push(identifier);
                }
            }
        }

        debug!(
            "Found {} identifiers for {} names across {} kinds with {} excluded containers",
            results.len(),
            names.len(),
            kinds.len(),
            excluded_container_ids.len()
        );
        Ok(results)
    }

    /// Check which (file_path, name) pairs have matching identifiers.
    ///
    /// Returns a HashSet of (file_path, name) pairs that exist in the identifiers table.
    /// Used by the resolver to detect if a caller file references a candidate's parent type.
    /// Chunked in batches of 250 per axis (250 files × 250 names = 500 params per query).
    pub fn get_identifier_presence(
        &self,
        file_paths: &[&str],
        names: &[&str],
    ) -> Result<HashSet<(String, String)>> {
        if file_paths.is_empty() || names.is_empty() {
            return Ok(HashSet::new());
        }

        // Each query uses file_chunk.len() + name_chunk.len() bind params.
        // Keep total under 500 with 250 per axis.
        const AXIS_CHUNK: usize = 250;
        let mut results = HashSet::new();

        for file_chunk in file_paths.chunks(AXIS_CHUNK) {
            for name_chunk in names.chunks(AXIS_CHUNK) {
                let file_placeholders: String = (1..=file_chunk.len())
                    .map(|i| format!("?{}", i))
                    .collect::<Vec<_>>()
                    .join(",");
                let name_offset = file_chunk.len();
                let name_placeholders: String = (1..=name_chunk.len())
                    .map(|i| format!("?{}", name_offset + i))
                    .collect::<Vec<_>>()
                    .join(",");

                let query = format!(
                    "SELECT DISTINCT file_path, name FROM identifiers \
                     WHERE file_path IN ({}) AND name IN ({})",
                    file_placeholders, name_placeholders
                );

                let mut stmt = self.conn.prepare(&query)?;

                let mut params: Vec<&dyn rusqlite::types::ToSql> = Vec::new();
                for fp in file_chunk {
                    params.push(fp as &dyn rusqlite::types::ToSql);
                }
                for n in name_chunk {
                    params.push(n as &dyn rusqlite::types::ToSql);
                }

                let rows = stmt.query_map(&*params, |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;

                for row in rows {
                    let (file_path, name) = row?;
                    results.insert((file_path, name));
                }
            }
        }

        debug!(
            "Identifier presence check: {} files x {} names → {} matches",
            file_paths.len(),
            names.len(),
            results.len()
        );
        Ok(results)
    }

    /// Check which files have at least one identifier in the database.
    ///
    /// Used to distinguish "we checked and found no match" from "we have no data"
    /// when applying negative filtering for phantom call edges.
    /// Chunked in batches of 500 to stay within SQLite's bind parameter limit.
    pub fn has_identifiers_for_files(&self, file_paths: &[&str]) -> Result<HashSet<String>> {
        if file_paths.is_empty() {
            return Ok(HashSet::new());
        }

        const CHUNK_SIZE: usize = 500;
        let mut results = HashSet::new();

        for chunk in file_paths.chunks(CHUNK_SIZE) {
            let placeholders: String = (1..=chunk.len())
                .map(|i| format!("?{}", i))
                .collect::<Vec<_>>()
                .join(",");

            let query = format!(
                "SELECT DISTINCT file_path FROM identifiers WHERE file_path IN ({})",
                placeholders
            );

            let mut stmt = self.conn.prepare(&query)?;
            let params: Vec<&dyn rusqlite::types::ToSql> = chunk
                .iter()
                .map(|fp| fp as &dyn rusqlite::types::ToSql)
                .collect();

            let rows = stmt.query_map(&*params, |row| row.get::<_, String>(0))?;
            for row in rows {
                results.insert(row?);
            }
        }

        Ok(results)
    }
}
