// Relationship operations for tracing data flow

use super::*;
use anyhow::Result;
use rusqlite::params;
use tracing::debug;

impl SymbolDatabase {
    pub fn delete_relationships_for_file(&self, file_path: &str) -> Result<()> {
        // Delete relationships where either the from_symbol or to_symbol belongs to the file
        let count = self.conn.execute(
            "DELETE FROM relationships
             WHERE from_symbol_id IN (
                 SELECT id FROM symbols WHERE file_path = ?1
             )
             OR to_symbol_id IN (
                 SELECT id FROM symbols WHERE file_path = ?1
             )",
            params![file_path],
        )?;

        debug!("Deleted {} relationships for file '{}'", count, file_path);
        Ok(())
    }

    pub fn get_outgoing_relationships(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
             FROM relationships
             WHERE from_symbol_id = ?1",
        )?;

        let rel_iter = stmt.query_map(params![symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for rel_result in rel_iter {
            relationships.push(rel_result?);
        }

        debug!(
            "Found {} outgoing relationships from symbol '{}'",
            relationships.len(),
            symbol_id
        );
        Ok(relationships)
    }

    /// Get all relationships for a symbol
    pub fn get_relationships_for_symbol(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
            FROM relationships
            WHERE from_symbol_id = ?1
        ",
        )?;

        let rows = stmt.query_map([symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for row_result in rows {
            relationships.push(row_result?);
        }

        Ok(relationships)
    }

    /// Get relationships TO a symbol (where symbol is the target/referenced)
    /// Uses indexed query on to_symbol_id for O(log n) performance
    /// Complements get_relationships_for_symbol() which finds relationships FROM a symbol
    pub fn get_relationships_to_symbol(&self, symbol_id: &str) -> Result<Vec<Relationship>> {
        let mut stmt = self.conn.prepare(
            "
            SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
            FROM relationships
            WHERE to_symbol_id = ?1
        ",
        )?;

        let rows = stmt.query_map([symbol_id], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for row_result in rows {
            relationships.push(row_result?);
        }

        Ok(relationships)
    }

    /// Get relationships TO multiple symbols in a single batch query
    /// PERFORMANCE FIX: Replaces N+1 query pattern with single batch query using SQL IN clause
    pub fn get_relationships_to_symbols(&self, symbol_ids: &[String]) -> Result<Vec<Relationship>> {
        if symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build parameterized query with IN clause for batch fetch
        let placeholders: Vec<String> = (1..=symbol_ids.len()).map(|i| format!("?{}", i)).collect();
        let query = format!(
            "SELECT id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata
             FROM relationships
             WHERE to_symbol_id IN ({})",
            placeholders.join(", ")
        );

        let mut stmt = self.conn.prepare(&query)?;

        // Convert Vec<String> to Vec<&dyn ToSql> for params
        let params: Vec<&dyn rusqlite::ToSql> = symbol_ids
            .iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();

        let relationship_iter = stmt.query_map(&params[..], |row| self.row_to_relationship(row))?;

        let mut relationships = Vec::new();
        for relationship_result in relationship_iter {
            relationships.push(relationship_result?);
        }

        Ok(relationships)
    }

    pub fn get_file_relationship_statistics(
        &self,
    ) -> Result<std::collections::HashMap<String, usize>> {
        use std::collections::HashMap;

        let mut by_file = HashMap::new();

        // This is a more complex query: count relationships per file
        // We need to join symbols with relationships to count how many relationships involve symbols from each file
        let rel_query = "SELECT s.file_path, COUNT(DISTINCT r.id) as count \
                         FROM symbols s \
                         LEFT JOIN relationships r ON (r.from_symbol_id = s.id OR r.to_symbol_id = s.id) \
                         GROUP BY s.file_path";

        let mut stmt = self.conn.prepare(rel_query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?;

        for row in rows {
            let (file_path, count) = row?;
            by_file.insert(file_path, count);
        }

        Ok(by_file)
    }

    /// Get relationship type statistics using SQL aggregation (avoids loading all relationships into memory)
    /// Returns HashMap<relationship_kind, count> grouped by relationship type
    /// Used by FastExploreTool's intelligent_dependencies mode
    pub fn get_relationship_type_statistics(&self) -> Result<HashMap<String, i64>> {
        let mut by_kind = HashMap::new();

        // SQL GROUP BY aggregation - counts relationships by kind without loading data into memory
        let query = "SELECT kind, COUNT(*) as count \
                     FROM relationships \
                     GROUP BY kind";

        let mut stmt = self.conn.prepare(query)?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        for row in rows {
            let (kind, count) = row?;
            by_kind.insert(kind, count);
        }

        Ok(by_kind)
    }
}
