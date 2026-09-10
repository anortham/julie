//! Short read queries over `facts.sqlite`. Rows are joined to `paths` at read
//! time; no transaction is held between calls.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use julie_extractors::{IdentifierKind, RelationshipKind, SymbolKind, Visibility};
use rusqlite::{Connection, OptionalExtension, Row, ToSql, params};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::rows::{
    ComplexityRow, DiagnosticRow, EncoderRow, IdentifierRow, PathRow, RelationshipRow,
    SourceRegionRow, Span, StructuralFactQuery, StructuralFactRow, SymbolRow, TypeRow, VectorRow,
    diagnostic_kind_from_str,
};
use crate::writer::encoder_row;

pub struct FactsReader<'a> {
    conn: &'a Connection,
    path: Option<&'a Path>,
}

fn from_json<T: DeserializeOwned>(text: Option<String>) -> rusqlite::Result<Option<T>> {
    text.map(|t| serde_json::from_str(&t))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
}

fn metadata(text: Option<String>) -> rusqlite::Result<Option<HashMap<String, Value>>> {
    from_json(text)
}

fn span_at(row: &Row<'_>, first: usize) -> rusqlite::Result<Span> {
    Ok(Span {
        start_line: row.get(first)?,
        start_col: row.get(first + 1)?,
        end_line: row.get(first + 2)?,
        end_col: row.get(first + 3)?,
        start_byte: row.get(first + 4)?,
        end_byte: row.get(first + 5)?,
    })
}

fn optional_span_at(row: &Row<'_>, first: usize) -> rusqlite::Result<Option<Span>> {
    let start_line: Option<u32> = row.get(first)?;
    if start_line.is_none() {
        return Ok(None);
    }
    span_at(row, first).map(Some)
}

fn placeholders(count: usize) -> String {
    (1..=count)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

const SPAN: &str = "start_line, start_col, end_line, end_col, start_byte, end_byte";

impl<'a> FactsReader<'a> {
    pub(crate) fn new(conn: &'a Connection, path: Option<&'a Path>) -> Self {
        Self { conn, path }
    }

    fn for_paths<T>(
        &self,
        paths: &[&str],
        sql: &str,
        map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let sql = sql.replace("{paths}", &placeholders(paths.len()));
        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn ToSql> = paths.iter().map(|p| p as &dyn ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), map)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn symbols_for_paths(&self, paths: &[&str]) -> Result<Vec<SymbolRow>> {
        self.for_paths(
            paths,
            &format!(
                "SELECT p.path, p.language, s.blob_hash, s.ordinal, s.name, s.kind, {SPAN},
                        body_start_line, body_start_col, body_end_line, body_end_col, body_start_byte, body_end_byte,
                        body_hash, signature, doc_comment, visibility, parent_ordinal, annotations, metadata,
                        semantic_group, confidence, content_type
                 FROM paths p JOIN symbols s ON s.blob_hash = p.blob_hash
                 WHERE p.path IN ({{paths}}) ORDER BY p.path, s.ordinal"
            ),
            |row| {
                let blob_hash: String = row.get(2)?;
                let ordinal: u32 = row.get(3)?;
                let kind: String = row.get(5)?;
                let visibility: Option<String> = row.get(21)?;
                Ok(SymbolRow {
                    id: format!("{blob_hash}:{ordinal}"),
                    path: row.get(0)?,
                    language: row.get(1)?,
                    blob_hash,
                    ordinal,
                    name: row.get(4)?,
                    kind: SymbolKind::from_string(&kind),
                    span: span_at(row, 6)?,
                    body_span: optional_span_at(row, 12)?,
                    body_hash: row.get(18)?,
                    signature: row.get(19)?,
                    doc_comment: row.get(20)?,
                    visibility: visibility.as_deref().and_then(Visibility::from_storage_str),
                    parent_ordinal: row.get(22)?,
                    annotations: from_json(row.get(23)?)?.unwrap_or_default(),
                    metadata: metadata(row.get(24)?)?,
                    semantic_group: row.get(25)?,
                    confidence: row.get(26)?,
                    content_type: row.get(27)?,
                })
            },
        )
    }

    pub fn identifiers_for_paths(&self, paths: &[&str]) -> Result<Vec<IdentifierRow>> {
        self.for_paths(
            paths,
            &format!(
                "SELECT p.path, p.language, i.blob_hash, i.ordinal, i.name, i.kind, {SPAN},
                        containing_ordinal, receiver_type, code_context, confidence
                 FROM paths p JOIN identifiers i ON i.blob_hash = p.blob_hash
                 WHERE p.path IN ({{paths}}) ORDER BY p.path, i.ordinal"
            ),
            |row| {
                let blob_hash: String = row.get(2)?;
                let ordinal: u32 = row.get(3)?;
                let kind: String = row.get(5)?;
                Ok(IdentifierRow {
                    id: format!("{blob_hash}:{ordinal}"),
                    path: row.get(0)?,
                    language: row.get(1)?,
                    blob_hash,
                    ordinal,
                    name: row.get(4)?,
                    kind: IdentifierKind::from_string(&kind),
                    span: span_at(row, 6)?,
                    containing_ordinal: row.get(12)?,
                    receiver_type: row.get(13)?,
                    code_context: row.get(14)?,
                    confidence: row.get(15)?,
                })
            },
        )
    }

    pub fn relationships_for_paths(&self, paths: &[&str]) -> Result<Vec<RelationshipRow>> {
        self.for_paths(
            paths,
            "SELECT p.path, r.blob_hash, r.ordinal, from_ordinal, to_name, to_blob_hash, to_ordinal,
                    kind, line_number, span, reference_site_is_exact, confidence, metadata
             FROM paths p JOIN relationships r ON r.blob_hash = p.blob_hash
             WHERE p.path IN ({paths}) ORDER BY p.path, r.ordinal",
            |row| {
                let kind: String = row.get(7)?;
                let span: Option<julie_extractors::NormalizedSpan> = from_json(row.get(9)?)?;
                Ok(RelationshipRow {
                    path: row.get(0)?,
                    blob_hash: row.get(1)?,
                    ordinal: row.get(2)?,
                    from_ordinal: row.get(3)?,
                    to_name: row.get(4)?,
                    to_blob_hash: row.get(5)?,
                    to_ordinal: row.get(6)?,
                    kind: RelationshipKind::from_string(&kind),
                    line_number: row.get(8)?,
                    span: span.map(Span::from),
                    reference_site_is_exact: row.get(10)?,
                    confidence: row.get(11)?,
                    metadata: metadata(row.get(12)?)?,
                })
            },
        )
    }

    pub fn types_for_paths(&self, paths: &[&str]) -> Result<Vec<TypeRow>> {
        self.for_paths(
            paths,
            "SELECT p.path, t.blob_hash, symbol_ordinal, resolved_type, generic_params, constraints, is_inferred
             FROM paths p JOIN types t ON t.blob_hash = p.blob_hash
             WHERE p.path IN ({paths}) ORDER BY p.path, symbol_ordinal",
            |row| {
                Ok(TypeRow {
                    path: row.get(0)?,
                    blob_hash: row.get(1)?,
                    symbol_ordinal: row.get(2)?,
                    resolved_type: row.get(3)?,
                    generic_params: from_json(row.get(4)?)?,
                    constraints: from_json(row.get(5)?)?,
                    is_inferred: row.get(6)?,
                })
            },
        )
    }

    pub fn source_regions_for_path(&self, path: &str) -> Result<Vec<SourceRegionRow>> {
        self.for_paths(
            &[path],
            &format!(
                "SELECT p.path, r.blob_hash, r.ordinal, kind, containing_ordinal, {SPAN}, metadata
                 FROM paths p JOIN source_regions r ON r.blob_hash = p.blob_hash
                 WHERE p.path IN ({{paths}}) ORDER BY r.ordinal"
            ),
            |row| {
                Ok(SourceRegionRow {
                    path: row.get(0)?,
                    blob_hash: row.get(1)?,
                    ordinal: row.get(2)?,
                    kind: row.get(3)?,
                    containing_ordinal: row.get(4)?,
                    span: span_at(row, 5)?,
                    metadata: metadata(row.get(11)?)?,
                })
            },
        )
    }

    pub fn structural_facts(&self, query: &StructuralFactQuery) -> Result<Vec<StructuralFactRow>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }
        let mut sql = format!(
            "SELECT p.path, p.language, f.blob_hash, f.ordinal, pattern_id, capture_name, node_kind,
                    containing_ordinal, {SPAN}, confidence, metadata
             FROM paths p JOIN structural_facts f ON f.blob_hash = p.blob_hash WHERE 1 = 1"
        );
        let mut values: Vec<Box<dyn ToSql>> = Vec::new();
        if !query.pattern_ids.is_empty() {
            let start = values.len() + 1;
            let marks: Vec<String> = (0..query.pattern_ids.len())
                .map(|i| format!("?{}", start + i))
                .collect();
            sql.push_str(&format!(" AND pattern_id IN ({})", marks.join(", ")));
            values.extend(
                query
                    .pattern_ids
                    .iter()
                    .map(|p| Box::new(p.clone()) as Box<dyn ToSql>),
            );
        }
        if let Some(pattern) = &query.path_pattern {
            sql.push_str(&format!(" AND p.path GLOB ?{}", values.len() + 1));
            values.push(Box::new(pattern.clone()));
        }
        if let Some(language) = &query.language {
            sql.push_str(&format!(" AND p.language = ?{}", values.len() + 1));
            values.push(Box::new(language.clone()));
        }
        sql.push_str(&format!(
            " ORDER BY p.path, f.ordinal LIMIT ?{}",
            values.len() + 1
        ));
        values.push(Box::new(query.limit as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let params: Vec<&dyn ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok(StructuralFactRow {
                path: row.get(0)?,
                language: row.get(1)?,
                blob_hash: row.get(2)?,
                ordinal: row.get(3)?,
                pattern_id: row.get(4)?,
                capture_name: row.get(5)?,
                node_kind: row.get(6)?,
                containing_ordinal: row.get(7)?,
                span: span_at(row, 8)?,
                confidence: row.get(14)?,
                metadata: metadata(row.get(15)?)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// `symbol_id` is `"<blob_hash>:<ordinal>"`; any other shape matches nothing.
    pub fn complexity_for_symbol(&self, symbol_id: &str) -> Result<Vec<ComplexityRow>> {
        let Some((blob_hash, ordinal)) = symbol_id.rsplit_once(':') else {
            return Ok(Vec::new());
        };
        let Ok(symbol_ordinal) = ordinal.parse::<u32>() else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT blob_hash, ordinal, scope, symbol_ordinal, algorithm_id, covered_lines, covered_bytes,
                    decision_count, loop_count, max_nesting_depth, parameter_count, {SPAN}, metadata
             FROM complexity_metrics WHERE blob_hash = ?1 AND symbol_ordinal = ?2 ORDER BY ordinal"
        ))?;
        let rows = stmt.query_map(params![blob_hash, symbol_ordinal], |row| {
            Ok(ComplexityRow {
                blob_hash: row.get(0)?,
                ordinal: row.get(1)?,
                scope: row.get(2)?,
                symbol_ordinal: row.get(3)?,
                algorithm_id: row.get(4)?,
                covered_lines: row.get(5)?,
                covered_bytes: row.get(6)?,
                decision_count: row.get(7)?,
                loop_count: row.get(8)?,
                max_nesting_depth: row.get(9)?,
                parameter_count: row.get(10)?,
                span: span_at(row, 11)?,
                metadata: metadata(row.get(17)?)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn diagnostics_for_path(&self, path: &str) -> Result<Vec<DiagnosticRow>> {
        self.for_paths(
            &[path],
            &format!(
                "SELECT p.path, d.blob_hash, d.ordinal, kind, message, {SPAN}
                 FROM paths p JOIN diagnostics d ON d.blob_hash = p.blob_hash
                 WHERE p.path IN ({{paths}}) ORDER BY d.ordinal"
            ),
            |row| {
                let kind: String = row.get(3)?;
                Ok(DiagnosticRow {
                    path: row.get(0)?,
                    blob_hash: row.get(1)?,
                    ordinal: row.get(2)?,
                    kind: diagnostic_kind_from_str(&kind),
                    message: row.get(4)?,
                    span: span_at(row, 5)?,
                })
            },
        )
    }

    pub fn paths(&self) -> Result<Vec<PathRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT path, blob_hash, language FROM paths ORDER BY path")?;
        let rows = stmt.query_map([], |row| {
            Ok(PathRow {
                path: row.get(0)?,
                blob_hash: row.get(1)?,
                language: row.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The encoder row, or `None` before any vectors were written.
    pub fn encoder(&self) -> Result<Option<EncoderRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, model_checksum, dimensions, pooling, normalization, instruction_policy FROM encoder",
                [],
                encoder_row,
            )
            .optional()?)
    }

    /// Every vector row for `encoder_id`, including rows for blobs no path
    /// holds right now (a reverted file reuses them).
    pub fn vectors_for_encoder(&self, encoder_id: &str) -> Result<Vec<VectorRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT blob_hash, symbol_ordinal, vector FROM vectors WHERE encoder_id = ?1 ORDER BY blob_hash, symbol_ordinal",
        )?;
        let rows = stmt.query_map([encoder_id], |row| {
            let bytes: Vec<u8> = row.get(2)?;
            Ok(VectorRow {
                blob_hash: row.get(0)?,
                symbol_ordinal: row.get(1)?,
                vector: VectorRow::vector_from_bytes(&bytes),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Vectors whose blob some path holds: the count the snapshot can serve.
    pub fn vector_count(&self) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM vectors v WHERE EXISTS (SELECT 1 FROM paths p WHERE p.blob_hash = v.blob_hash)",
            [],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    /// Symbols whose blob some path holds.
    pub fn symbol_count(&self) -> Result<u64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM symbols s WHERE EXISTS (SELECT 1 FROM paths p WHERE p.blob_hash = s.blob_hash)",
            [],
            |r| r.get::<_, i64>(0),
        )? as u64)
    }

    pub fn blob_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM blobs", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Size of the database file on disk; zero for an in-memory store.
    pub fn file_size_bytes(&self) -> Result<u64> {
        match self.path {
            Some(path) => Ok(std::fs::metadata(path)?.len()),
            None => Ok(0),
        }
    }
}
