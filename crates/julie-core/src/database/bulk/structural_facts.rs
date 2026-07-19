use std::collections::HashSet;

use anyhow::Result;
use julie_extractors::base::StructuralFact;
use rusqlite::{Transaction, params};

pub(crate) fn insert_structural_facts_tx(
    tx: &Transaction<'_>,
    facts: &[StructuralFact],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO structural_facts
         (id, file_path, language, pattern_id, capture_name, node_kind,
          containing_symbol_id, start_line, start_col, end_line, end_col,
          start_byte, end_byte, confidence, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )?;
    for fact in facts {
        let containing_symbol_id = fact
            .containing_symbol_id
            .as_deref()
            .filter(|id| valid_symbol_ids.is_none_or(|valid| valid.contains(*id)));
        let metadata = fact
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        stmt.execute(params![
            fact.id,
            fact.file_path,
            fact.language,
            fact.pattern_id,
            fact.capture_name,
            fact.node_kind,
            containing_symbol_id,
            fact.start_line,
            fact.start_column,
            fact.end_line,
            fact.end_column,
            fact.start_byte,
            fact.end_byte,
            fact.confidence,
            metadata,
        ])?;
    }
    Ok(facts.len() as i64)
}
