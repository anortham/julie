//! Derived web navigation edges produced from structural facts.
//!
//! Unlike `Relationship` (owned by the external `julie-extractors` crate with
//! a closed `RelationshipKind` enum), web edges are *derived* data: Julie's
//! indexing pipeline joins structural facts into navigation edges. The type
//! and its storage therefore live here in julie-core, not in the extractor.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::types::Type;
use rusqlite::{params, params_from_iter};
use tracing::debug;

use super::SymbolDatabase;
use crate::database::bulk::web_edges::insert_web_edges_tx;

/// Kind of derived web navigation edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WebEdgeKind {
    /// Frontend HTTP client call -> backend route handler (HTTP boundary).
    HttpCall,
    /// Backend query -> SQL table.
    SqlQuery,
}

impl WebEdgeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WebEdgeKind::HttpCall => "http_call",
            WebEdgeKind::SqlQuery => "sql_query",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "http_call" => Some(WebEdgeKind::HttpCall),
            "sql_query" => Some(WebEdgeKind::SqlQuery),
            _ => None,
        }
    }
}

/// A derived navigation edge joining two symbols (or a symbol and an external
/// endpoint / table) via a web structural fact.
#[derive(Debug, Clone, PartialEq)]
pub struct WebEdge {
    /// Symbol that owns the client-call / query fact (edge origin).
    pub from_symbol_id: String,
    /// Resolved in-workspace target symbol (route handler / table). `None`
    /// when no in-workspace symbol matched -> see `to_external`.
    pub to_symbol_id: Option<String>,
    /// External target when no in-workspace symbol matched
    /// (e.g. `"GET /api/foo"` or `"table:users"`).
    pub to_external: Option<String>,
    pub kind: WebEdgeKind,
    /// HTTP method, when relevant.
    pub method: Option<String>,
    /// Route / request path, when relevant.
    pub path: Option<String>,
    /// SQL table, when relevant.
    pub table: Option<String>,
    /// Origin file of the client-call / query fact (for call-site links).
    pub file_path: String,
    /// Origin line of the client-call / query fact (1-based).
    pub line_number: u32,
    /// Combined confidence (min of source + target fact confidences).
    pub confidence: f32,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl SymbolDatabase {
    /// Replace the entire `web_edges` table with `edges`. Used by the
    /// post-persistence rebuild pass: the table is derived data, so it is
    /// wiped and recomputed from `structural_facts` on each rebuild.
    pub fn replace_all_web_edges(&mut self, edges: &[WebEdge]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM web_edges", [])?;
        let inserted = insert_web_edges_tx(&tx, edges, None)?;
        tx.commit()?;
        debug!("Replaced web_edges table: {} edges inserted", inserted);
        Ok(())
    }

    /// Forward edges originating at `from_symbol_id` (trace: "what does this
    /// symbol call / query?").
    pub fn web_edges_from_symbol(&self, from_symbol_id: &str) -> Result<Vec<WebEdge>> {
        self.web_edges_from_symbols(&[from_symbol_id.to_string()])
    }

    /// Batch forward lookup for graph walks (avoids N+1 in web-mode BFS).
    /// Chunked like `get_outgoing_relationships_for_symbols`.
    pub fn web_edges_from_symbols(&self, from_symbol_ids: &[String]) -> Result<Vec<WebEdge>> {
        if from_symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        const CHUNK_SIZE: usize = 500;
        let mut edges = Vec::new();

        for chunk in from_symbol_ids.chunks(CHUNK_SIZE) {
            let placeholders = (1..=chunk.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT from_symbol_id, to_symbol_id, to_external, kind, method, path,
                        table_name, file_path, line_number, confidence, metadata
                 FROM web_edges
                 WHERE from_symbol_id IN ({placeholders})
                 ORDER BY from_symbol_id, kind, file_path, line_number"
            );
            let params: Vec<&dyn rusqlite::ToSql> =
                chunk.iter().map(|id| id as &dyn rusqlite::ToSql).collect();
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(params), row_to_web_edge)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            edges.extend(rows);
        }

        Ok(edges)
    }

    /// Reverse edges pointing at any of `to_symbol_ids` (impact: "who calls
    /// this endpoint / queries this table?"). Batch lookup for graph walks.
    pub fn web_edges_to_symbols(&self, to_symbol_ids: &[String]) -> Result<Vec<WebEdge>> {
        if to_symbol_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=to_symbol_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT from_symbol_id, to_symbol_id, to_external, kind, method, path,
                    table_name, file_path, line_number, confidence, metadata
             FROM web_edges
             WHERE to_symbol_id IN ({placeholders})"
        );
        let params: Vec<&dyn rusqlite::ToSql> = to_symbol_ids
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(params), row_to_web_edge)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// All edges of a given kind (used by diagnostics / search boosting).
    pub fn web_edges_of_kind(&self, kind: WebEdgeKind) -> Result<Vec<WebEdge>> {
        let mut stmt = self.conn.prepare(
            "SELECT from_symbol_id, to_symbol_id, to_external, kind, method, path,
                    table_name, file_path, line_number, confidence, metadata
             FROM web_edges
             WHERE kind = ?1
             ORDER BY file_path, line_number",
        )?;
        let rows = stmt
            .query_map(params![kind.as_str()], row_to_web_edge)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Count of derived web edges (lightweight health signal).
    pub fn web_edge_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM web_edges", [], |row| row.get(0))?)
    }
}

fn row_to_web_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<WebEdge> {
    let kind_str: String = row.get(3)?;
    let kind = WebEdgeKind::from_str(&kind_str).unwrap_or(WebEdgeKind::HttpCall);
    let metadata_json: Option<String> = row.get(10)?;
    let metadata = metadata_json
        .map(|value| serde_json::from_str(&value))
        .transpose()
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err)))?;
    Ok(WebEdge {
        from_symbol_id: row.get(0)?,
        to_symbol_id: row.get(1)?,
        to_external: row.get(2)?,
        kind,
        method: row.get(4)?,
        path: row.get(5)?,
        table: row.get(6)?,
        file_path: row.get(7)?,
        line_number: row.get(8)?,
        confidence: row.get(9)?,
        metadata,
    })
}
