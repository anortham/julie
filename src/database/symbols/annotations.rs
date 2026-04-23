use anyhow::Result;
use rusqlite::{Connection, Params, Statement, Transaction, params, params_from_iter};
use std::collections::HashMap;

use crate::database::SymbolDatabase;
use crate::extractors::{AnnotationMarker, Symbol};

const CHUNK_SIZE: usize = 500;

pub(in crate::database) trait AnnotationConnection {
    fn execute_annotation<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize>;
    fn prepare_annotation(&self, sql: &str) -> rusqlite::Result<Statement<'_>>;
}

impl AnnotationConnection for Connection {
    fn execute_annotation<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        self.execute(sql, params)
    }

    fn prepare_annotation(&self, sql: &str) -> rusqlite::Result<Statement<'_>> {
        self.prepare(sql)
    }
}

impl AnnotationConnection for Transaction<'_> {
    fn execute_annotation<P: Params>(&self, sql: &str, params: P) -> rusqlite::Result<usize> {
        self.execute(sql, params)
    }

    fn prepare_annotation(&self, sql: &str) -> rusqlite::Result<Statement<'_>> {
        self.prepare(sql)
    }
}

pub(in crate::database) fn replace_annotations_batch<C: AnnotationConnection>(
    conn: &C,
    symbols: &[Symbol],
) -> Result<()> {
    if symbols.is_empty() {
        return Ok(());
    }

    delete_annotations_for_symbols(conn, symbols)?;

    let mut stmt = conn.prepare_annotation(
        "INSERT INTO symbol_annotations
         (id, symbol_id, ordinal, annotation, annotation_key, raw_text, carrier)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;

    for symbol in symbols {
        for (ordinal, marker) in symbol.annotations.iter().enumerate() {
            stmt.execute(params![
                annotation_row_id(&symbol.id, ordinal),
                symbol.id,
                ordinal as i64,
                marker.annotation,
                marker.annotation_key,
                marker.raw_text.as_deref(),
                marker.carrier.as_deref(),
            ])?;
        }
    }

    Ok(())
}

pub(in crate::database) fn delete_annotations_for_file<C: AnnotationConnection>(
    conn: &C,
    file_path: &str,
) -> Result<()> {
    conn.execute_annotation(
        "DELETE FROM symbol_annotations
         WHERE symbol_id IN (SELECT id FROM symbols WHERE file_path = ?1)",
        params![file_path],
    )?;
    Ok(())
}

fn delete_annotations_for_symbols<C: AnnotationConnection>(
    conn: &C,
    symbols: &[Symbol],
) -> Result<()> {
    for chunk in symbols.chunks(CHUNK_SIZE) {
        let placeholders = placeholders(chunk.len());
        let sql = format!(
            "DELETE FROM symbol_annotations WHERE symbol_id IN ({})",
            placeholders
        );
        let params = chunk.iter().map(|symbol| symbol.id.as_str());
        conn.execute_annotation(&sql, params_from_iter(params))?;
    }
    Ok(())
}

pub(in crate::database) fn hydrate_annotations_for_symbols(
    db: &SymbolDatabase,
    symbols: &mut [Symbol],
) -> Result<()> {
    if symbols.is_empty() {
        return Ok(());
    }

    let mut annotations_by_symbol: HashMap<String, Vec<AnnotationMarker>> = HashMap::new();

    for chunk in symbols.chunks(CHUNK_SIZE) {
        let placeholders = placeholders(chunk.len());
        let sql = format!(
            "SELECT symbol_id, annotation, annotation_key, raw_text, carrier
             FROM symbol_annotations
             WHERE symbol_id IN ({})
             ORDER BY symbol_id, ordinal",
            placeholders
        );
        let mut stmt = db.conn.prepare(&sql)?;
        let ids = chunk.iter().map(|symbol| symbol.id.as_str());
        let rows = stmt.query_map(params_from_iter(ids), |row| {
            Ok((
                row.get::<_, String>(0)?,
                AnnotationMarker {
                    annotation: row.get(1)?,
                    annotation_key: row.get(2)?,
                    raw_text: row.get(3)?,
                    carrier: row.get(4)?,
                },
            ))
        })?;

        for row in rows {
            let (symbol_id, marker) = row?;
            annotations_by_symbol
                .entry(symbol_id)
                .or_default()
                .push(marker);
        }
    }

    for symbol in symbols {
        symbol.annotations = annotations_by_symbol.remove(&symbol.id).unwrap_or_default();
    }

    Ok(())
}

fn annotation_row_id(symbol_id: &str, ordinal: usize) -> String {
    format!("{symbol_id}:{ordinal}")
}

fn placeholders(len: usize) -> String {
    (1..=len)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",")
}
