// Identifier query operations - unlocking the identifiers table for fast_refs
//
// The identifiers table stores every usage site extracted by all 31 language extractors.
// Each row represents a place where a symbol name appears in code (calls, type usages,
// member access, imports, variable references). This module provides read access to
// that data, which fast_refs uses to find references beyond what the relationships
// table captures.

use super::*;
use anyhow::Result;
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
        prefix_conditions.push(format!("name LIKE ?{}", idx));
        params.push(Box::new(format!("{}::%", name)));
        idx += 1;
        // Dot-style qualifier (most other languages)
        prefix_conditions.push(format!("name LIKE ?{}", idx));
        params.push(Box::new(format!("{}.%", name)));
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
}
