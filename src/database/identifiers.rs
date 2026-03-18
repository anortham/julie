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
    pub confidence: f32,
}

/// Column list for IdentifierRef queries
const IDENTIFIER_REF_COLUMNS: &str =
    "name, kind, file_path, start_line, containing_symbol_id, confidence";

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
            confidence: row.get("confidence")?,
        })
    }

    /// Find all identifiers matching any of the given names or their qualified forms.
    /// Matches both exact names ("CodeTokenizer") and qualified calls ("CodeTokenizer::new").
    /// Uses idx_identifiers_name index for exact matches; LIKE for prefix matches.
    pub fn get_identifiers_by_names(&self, names: &[String]) -> Result<Vec<IdentifierRef>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        let (where_clause, params) = build_name_match_clause(names);
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

        let mut results = Vec::new();
        for row_result in rows {
            results.push(row_result?);
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
    /// Used by security risk analysis for batch sink detection.
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

        debug!(
            "Loaded {} call identifiers across {} symbols",
            grouped.values().map(|v| v.len()).sum::<usize>(),
            grouped.len()
        );

        Ok(grouped)
    }

    /// Find identifiers matching any of the given names or qualified forms, filtered by kind.
    /// Used for reference_kind filtering in fast_refs.
    pub fn get_identifiers_by_names_and_kind(
        &self,
        names: &[String],
        kind: &str,
    ) -> Result<Vec<IdentifierRef>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        let (where_clause, mut params) = build_name_match_clause(names);
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

        let mut results = Vec::new();
        for row_result in rows {
            results.push(row_result?);
        }

        debug!(
            "Found {} identifiers for {} name variants with kind='{}' (with prefix matching)",
            results.len(),
            names.len(),
            kind
        );
        Ok(results)
    }

    /// Check which (file_path, name) pairs have matching identifiers.
    ///
    /// Returns a HashSet of (file_path, name) pairs that exist in the identifiers table.
    /// Used by the resolver to detect if a caller file references a candidate's parent type.
    pub fn get_identifier_presence(
        &self,
        file_paths: &[&str],
        names: &[&str],
    ) -> Result<HashSet<(String, String)>> {
        if file_paths.is_empty() || names.is_empty() {
            return Ok(HashSet::new());
        }

        let file_placeholders: String = (1..=file_paths.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(",");
        let name_offset = file_paths.len();
        let name_placeholders: String = (1..=names.len())
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
        for fp in file_paths {
            params.push(fp as &dyn rusqlite::types::ToSql);
        }
        for n in names {
            params.push(n as &dyn rusqlite::types::ToSql);
        }

        let mut results = HashSet::new();
        let rows = stmt.query_map(&*params, |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (file_path, name) = row?;
            results.insert((file_path, name));
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
    pub fn has_identifiers_for_files(
        &self,
        file_paths: &[&str],
    ) -> Result<HashSet<String>> {
        if file_paths.is_empty() {
            return Ok(HashSet::new());
        }

        let placeholders: String = (1..=file_paths.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(",");

        let query = format!(
            "SELECT DISTINCT file_path FROM identifiers WHERE file_path IN ({})",
            placeholders
        );

        let mut stmt = self.conn.prepare(&query)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = file_paths
            .iter()
            .map(|fp| fp as &dyn rusqlite::types::ToSql)
            .collect();

        let mut results = HashSet::new();
        let rows = stmt.query_map(&*params, |row| row.get::<_, String>(0))?;

        for row in rows {
            results.insert(row?);
        }

        Ok(results)
    }
}
