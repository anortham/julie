use std::collections::HashSet;

use anyhow::Result;
use rusqlite::{params, Transaction};
use tracing::debug;

use crate::database::web_edges::WebEdge;

/// Deterministic primary key for a derived web edge. Re-deriving the same
/// edge from the same facts yields the same id, so `INSERT OR REPLACE` is
/// idempotent across reindexes. Includes `path` and `table` so two edges that
/// share from/to/file/line/method but differ in route or SQL target do not
/// collide and silently drop under `INSERT OR REPLACE`.
fn edge_id(edge: &WebEdge) -> String {
    let to = edge
        .to_symbol_id
        .clone()
        .or_else(|| edge.to_external.clone())
        .unwrap_or_else(|| "?".into());
    format!(
        "web-edge:{}:{}:{}:{}:{}:{}:{}:{}",
        edge.kind.as_str(),
        edge.from_symbol_id,
        to,
        edge.file_path,
        edge.line_number,
        edge.method.as_deref().unwrap_or(""),
        edge.path.as_deref().unwrap_or(""),
        edge.table.as_deref().unwrap_or(""),
    )
}

pub(crate) fn insert_web_edges_tx(
    tx: &Transaction<'_>,
    edges: &[WebEdge],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    if edges.is_empty() {
        return Ok(0);
    }

    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO web_edges
         (id, from_symbol_id, to_symbol_id, to_external, kind, method, path,
          table_name, file_path, line_number, confidence, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;

    let mut inserted = 0;
    for edge in edges {
        // `from_symbol_id` must reference a real symbol; drop the edge if the
        // origin symbol wasn't persisted (mirrors relationships' guard).
        let from_ok = valid_symbol_ids
            .map(|valid| valid.contains(&edge.from_symbol_id))
            .unwrap_or(true);
        if !from_ok {
            debug!(
                "Skipping web edge from {} (missing symbol reference)",
                edge.from_symbol_id
            );
            continue;
        }
        // `to_symbol_id` is optional; when present it must reference a real
        // symbol, otherwise null it out (degrade to external).
        let to_symbol_id: Option<String> = match &edge.to_symbol_id {
            Some(id) => match valid_symbol_ids {
                Some(valid) if !valid.contains(id) => None,
                _ => Some(id.clone()),
            },
            None => None,
        };
        if to_symbol_id.is_none() && edge.to_external.is_none() {
            // Nothing to point at — skip rather than emit a dangling edge.
            continue;
        }

        let metadata_json = edge
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        stmt.execute(params![
            edge_id(edge),
            edge.from_symbol_id,
            to_symbol_id,
            edge.to_external,
            edge.kind.as_str(),
            edge.method,
            edge.path,
            edge.table,
            edge.file_path,
            edge.line_number,
            edge.confidence,
            metadata_json
        ])?;
        inserted += 1;
    }

    Ok(inserted)
}
