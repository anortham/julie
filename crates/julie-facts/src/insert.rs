//! Insert every fact row for one blob. Extractor ids become ordinals within
//! the blob; an id that points outside the blob becomes a name-only target.

use std::collections::HashMap;

use anyhow::Result;
use julie_extractors::{ExtractionResults, NormalizedSpan};
use rusqlite::{Transaction, params};
use serde_json::Value;

use crate::rows::{diagnostic_kind_str, flatten_type_argument_usages};

type Ordinals<'a> = HashMap<&'a str, u32>;

fn json(value: &impl serde::Serialize) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

fn json_opt(value: &Option<impl serde::Serialize>) -> Result<Option<String>> {
    value.as_ref().map(|v| json(v)).transpose()
}

fn metadata_json(value: &Option<HashMap<String, Value>>) -> Result<Option<String>> {
    json_opt(value)
}

fn ordinal_of(map: &Ordinals<'_>, id: Option<&str>) -> Option<u32> {
    id.and_then(|id| map.get(id).copied())
}

pub fn insert_blob_rows(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
) -> Result<()> {
    let symbol_ordinals: Ordinals<'_> = results
        .symbols
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i as u32))
        .collect();
    let identifier_ordinals: Ordinals<'_> = results
        .identifiers
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.as_str(), i as u32))
        .collect();

    insert_symbols(tx, blob_hash, results, &symbol_ordinals)?;
    insert_identifiers(tx, blob_hash, results, &symbol_ordinals)?;
    insert_relationships(tx, blob_hash, results, &symbol_ordinals)?;
    insert_types(tx, blob_hash, results, &symbol_ordinals)?;
    insert_source_regions(tx, blob_hash, results, &symbol_ordinals)?;
    insert_structural_facts(tx, blob_hash, results, &symbol_ordinals)?;
    insert_complexity(tx, blob_hash, results, &symbol_ordinals)?;
    insert_literals(tx, blob_hash, results, &symbol_ordinals)?;
    insert_type_arguments(tx, blob_hash, results, &identifier_ordinals)?;
    insert_diagnostics(tx, blob_hash, results)?;
    Ok(())
}

fn span_of(s: &NormalizedSpan) -> [u32; 6] {
    [
        s.start_line,
        s.start_column,
        s.end_line,
        s.end_column,
        s.start_byte,
        s.end_byte,
    ]
}

fn insert_symbols(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO symbols (blob_hash, ordinal, name, kind,
            start_line, start_col, end_line, end_col, start_byte, end_byte,
            body_start_line, body_start_col, body_end_line, body_end_col, body_start_byte, body_end_byte,
            body_hash, signature, doc_comment, visibility, parent_ordinal, annotations, metadata,
            semantic_group, confidence, content_type)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
    )?;
    for (ordinal, s) in results.symbols.iter().enumerate() {
        let body = s.body_span.as_ref().map(span_of);
        let body_col = |i: usize| body.map(|b| b[i]);
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            s.name,
            s.kind.to_string(),
            s.start_line,
            s.start_column,
            s.end_line,
            s.end_column,
            s.start_byte,
            s.end_byte,
            body_col(0),
            body_col(1),
            body_col(2),
            body_col(3),
            body_col(4),
            body_col(5),
            s.body_hash,
            s.signature,
            s.doc_comment,
            s.visibility.as_ref().map(|v| v.as_storage_str()),
            ordinal_of(symbols, s.parent_id.as_deref()),
            json(&s.annotations)?,
            metadata_json(&s.metadata)?,
            s.semantic_group,
            s.confidence,
            s.content_type,
        ])?;
    }
    Ok(())
}

fn insert_identifiers(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO identifiers (blob_hash, ordinal, name, kind,
            start_line, start_col, end_line, end_col, start_byte, end_byte,
            containing_ordinal, receiver_type, code_context, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;
    for (ordinal, i) in results.identifiers.iter().enumerate() {
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            i.name,
            i.kind.to_string(),
            i.start_line,
            i.start_column,
            i.end_line,
            i.end_column,
            i.start_byte,
            i.end_byte,
            ordinal_of(symbols, i.containing_symbol_id.as_deref()),
            i.receiver_type,
            i.code_context,
            i.confidence,
        ])?;
    }
    Ok(())
}

fn insert_relationships(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO relationships (blob_hash, ordinal, from_ordinal, to_name, to_blob_hash, to_ordinal,
            kind, line_number, span, reference_site_is_exact, confidence, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    let mut ordinal = 0u32;
    for r in &results.relationships {
        let to_ordinal = ordinal_of(symbols, Some(&r.to_symbol_id));
        let to_name = to_ordinal
            .map(|o| results.symbols[o as usize].name.clone())
            .unwrap_or_else(|| r.to_symbol_id.clone());
        stmt.execute(params![
            blob_hash,
            ordinal,
            ordinal_of(symbols, Some(&r.from_symbol_id)),
            to_name,
            to_ordinal.map(|_| blob_hash),
            to_ordinal,
            r.kind.to_string(),
            r.line_number,
            json_opt(&r.span)?,
            r.reference_site_is_exact,
            r.confidence,
            metadata_json(&r.metadata)?,
        ])?;
        ordinal += 1;
    }
    let qualified = qualified_pending_names(results);
    for p in &results.pending_relationships {
        let to_name = qualified
            .get(&(
                p.from_symbol_id.as_str(),
                p.line_number,
                p.callee_name.as_str(),
            ))
            .copied()
            .unwrap_or(p.callee_name.as_str());
        stmt.execute(params![
            blob_hash,
            ordinal,
            ordinal_of(symbols, Some(&p.from_symbol_id)),
            to_name,
            Option::<String>::None,
            Option::<u32>::None,
            p.kind.to_string(),
            p.line_number,
            Option::<String>::None,
            false,
            p.confidence,
            Option::<String>::None,
        ])?;
        ordinal += 1;
    }
    Ok(())
}

/// `(from id, line, leaf name)` -> qualified display name for pending rows
/// whose structured target carries a namespace path (`crate::a::b::leaf`).
/// Receiver-based targets (`obj.method`) keep the leaf.
fn qualified_pending_names(results: &ExtractionResults) -> HashMap<(&str, u32, &str), &str> {
    results
        .structured_pending_relationships
        .iter()
        .filter(|s| !s.target.namespace_path.is_empty() && s.target.receiver.is_none())
        .map(|s| {
            (
                (
                    s.pending.from_symbol_id.as_str(),
                    s.pending.line_number,
                    s.target.terminal_name.as_str(),
                ),
                s.target.display_name.as_str(),
            )
        })
        .collect()
}

fn insert_types(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO types (blob_hash, symbol_ordinal, resolved_type, generic_params, constraints, is_inferred)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for t in results.types.values() {
        let Some(symbol_ordinal) = ordinal_of(symbols, Some(&t.symbol_id)) else {
            continue;
        };
        stmt.execute(params![
            blob_hash,
            symbol_ordinal,
            t.resolved_type,
            json_opt(&t.generic_params)?,
            json_opt(&t.constraints)?,
            t.is_inferred,
        ])?;
    }
    Ok(())
}

fn insert_source_regions(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO source_regions (blob_hash, ordinal, kind, containing_ordinal,
            start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?;
    for (ordinal, r) in results.source_regions.iter().enumerate() {
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            r.kind.as_str(),
            ordinal_of(symbols, r.containing_symbol_id.as_deref()),
            r.start_line,
            r.start_column,
            r.end_line,
            r.end_column,
            r.start_byte,
            r.end_byte,
            metadata_json(&r.metadata)?,
        ])?;
    }
    Ok(())
}

fn insert_structural_facts(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO structural_facts (blob_hash, ordinal, pattern_id, capture_name, node_kind, containing_ordinal,
            start_line, start_col, end_line, end_col, start_byte, end_byte, confidence, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;
    for (ordinal, f) in results.structural_facts.iter().enumerate() {
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            f.pattern_id,
            f.capture_name,
            f.node_kind,
            ordinal_of(symbols, f.containing_symbol_id.as_deref()),
            f.start_line,
            f.start_column,
            f.end_line,
            f.end_column,
            f.start_byte,
            f.end_byte,
            f.confidence,
            metadata_json(&f.metadata)?,
        ])?;
    }
    Ok(())
}

fn insert_complexity(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO complexity_metrics (blob_hash, ordinal, scope, symbol_ordinal, algorithm_id,
            covered_lines, covered_bytes, decision_count, loop_count, max_nesting_depth, parameter_count,
            start_line, start_col, end_line, end_col, start_byte, end_byte, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
    )?;
    for (ordinal, m) in results.complexity_metrics.iter().enumerate() {
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            m.scope,
            ordinal_of(symbols, m.symbol_id.as_deref()),
            m.algorithm_id,
            m.covered_lines,
            m.covered_bytes,
            m.decision_count,
            m.loop_count,
            m.max_nesting_depth,
            m.parameter_count,
            m.start_line,
            m.start_column,
            m.end_line,
            m.end_column,
            m.start_byte,
            m.end_byte,
            metadata_json(&m.metadata)?,
        ])?;
    }
    Ok(())
}

fn insert_literals(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    symbols: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO literals (blob_hash, ordinal, literal_text, kind, carrier, arg_position, containing_ordinal,
            start_line, start_col, end_line, end_col, start_byte, end_byte, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;
    for (ordinal, l) in results.literals.iter().enumerate() {
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            l.literal_text,
            l.kind.as_str(),
            l.carrier,
            l.arg_position,
            ordinal_of(symbols, l.containing_symbol_id.as_deref()),
            l.start_line,
            l.start_column,
            l.end_line,
            l.end_column,
            l.start_byte,
            l.end_byte,
            l.confidence,
        ])?;
    }
    Ok(())
}

fn insert_type_arguments(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
    identifiers: &Ordinals<'_>,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO type_arguments (blob_hash, ordinal, identifier_ordinal, parent_ordinal, position, type_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )?;
    for (ordinal, a) in flatten_type_argument_usages(&results.type_argument_usages)
        .iter()
        .enumerate()
    {
        let Some(identifier_ordinal) = ordinal_of(identifiers, Some(&a.identifier_id)) else {
            continue;
        };
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            identifier_ordinal,
            a.parent.map(|p| p as u32),
            a.position,
            a.type_name,
        ])?;
    }
    Ok(())
}

fn insert_diagnostics(
    tx: &Transaction<'_>,
    blob_hash: &str,
    results: &ExtractionResults,
) -> Result<()> {
    let mut stmt = tx.prepare_cached(
        "INSERT INTO diagnostics (blob_hash, ordinal, kind, message,
            start_line, start_col, end_line, end_col, start_byte, end_byte)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for (ordinal, d) in results.parse_diagnostics.iter().enumerate() {
        stmt.execute(params![
            blob_hash,
            ordinal as u32,
            diagnostic_kind_str(d.kind),
            d.message,
            d.start_line,
            d.start_column,
            d.end_line,
            d.end_column,
            d.start_byte,
            d.end_byte,
        ])?;
    }
    Ok(())
}
