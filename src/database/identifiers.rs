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

    /// Find all identifiers matching any of the given names.
    /// Uses idx_identifiers_name index for O(log n) per name.
    /// Designed for batch queries with cross-language naming variants.
    pub fn get_identifiers_by_names(&self, names: &[String]) -> Result<Vec<IdentifierRef>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        let placeholders: Vec<String> = (1..=names.len()).map(|i| format!("?{}", i)).collect();
        let query = format!(
            "SELECT {} FROM identifiers WHERE name IN ({})",
            IDENTIFIER_REF_COLUMNS,
            placeholders.join(", ")
        );

        let mut stmt = self.conn.prepare(&query)?;

        let params: Vec<&dyn rusqlite::ToSql> =
            names.iter().map(|n| n as &dyn rusqlite::ToSql).collect();

        let rows = stmt.query_map(&params[..], |row| self.row_to_identifier_ref(row))?;

        let mut results = Vec::new();
        for row_result in rows {
            results.push(row_result?);
        }

        debug!(
            "Found {} identifiers for {} name variants",
            results.len(),
            names.len()
        );
        Ok(results)
    }

    /// Find identifiers matching any of the given names, filtered by kind.
    /// Used for reference_kind filtering in fast_refs.
    pub fn get_identifiers_by_names_and_kind(
        &self,
        names: &[String],
        kind: &str,
    ) -> Result<Vec<IdentifierRef>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }

        // N placeholders for names + 1 for kind
        let name_placeholders: Vec<String> =
            (1..=names.len()).map(|i| format!("?{}", i)).collect();
        let kind_placeholder = format!("?{}", names.len() + 1);

        let query = format!(
            "SELECT {} FROM identifiers WHERE name IN ({}) AND kind = {}",
            IDENTIFIER_REF_COLUMNS,
            name_placeholders.join(", "),
            kind_placeholder
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Build params: names + kind
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        for name in names {
            params.push(Box::new(name.clone()));
        }
        params.push(Box::new(kind.to_string()));

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
            "Found {} identifiers for {} name variants with kind='{}'",
            results.len(),
            names.len(),
            kind
        );
        Ok(results)
    }
}
