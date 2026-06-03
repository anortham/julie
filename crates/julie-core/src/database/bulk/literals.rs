//! Bulk persistence for string-literal call-args captured at carrier sites
//! (Miller bridge Phase 3). Mirrors `bulk/identifiers.rs`: early-return on
//! empty, `INSERT OR REPLACE`, run under the FK-disabled bulk window owned by
//! `atomic.rs`.
//!
//! Literals reaching this point are already carrier-classified-and-gated by
//! `classify_literals_by_carrier` (non-carrier literals dropped before the
//! write), so every row stored here has a recognized `kind` and a `carrier`.

use std::collections::HashSet;

use anyhow::Result;
use rusqlite::{Transaction, params};
use tracing::{debug, info};

use crate::database::SymbolDatabase;
use julie_extractors::Literal;

pub(crate) fn insert_literals_tx(
    tx: &Transaction<'_>,
    literals: &[Literal],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    if literals.is_empty() {
        return Ok(0);
    }

    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO literals
         (id, literal_text, kind, carrier, arg_position, language, file_path,
          start_line, start_col, end_line, end_col, start_byte, end_byte,
          containing_symbol_id, confidence)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )?;

    let mut inserted = 0;
    for literal in literals {
        let containing_symbol_id = normalize_symbol_ref(
            literal.containing_symbol_id.as_deref(),
            valid_symbol_ids,
            &literal.id,
        );

        stmt.execute(params![
            literal.id,
            literal.literal_text,
            literal.kind.as_str(),
            literal.carrier,
            literal.arg_position,
            literal.language,
            literal.file_path,
            literal.start_line,
            literal.start_column,
            literal.end_line,
            literal.end_column,
            literal.start_byte,
            literal.end_byte,
            containing_symbol_id,
            literal.confidence,
        ])?;
        inserted += 1;
    }

    Ok(inserted)
}

/// Drop a `containing_symbol_id` that is not present in the batch's known symbol
/// set (mirrors the identifier guard): a literal whose enclosing symbol was not
/// persisted stores NULL rather than dangling. With no validity set (positional
/// callers), the id is kept verbatim.
fn normalize_symbol_ref(
    symbol_id: Option<&str>,
    valid_symbol_ids: Option<&HashSet<String>>,
    literal_id: &str,
) -> Option<String> {
    match (symbol_id, valid_symbol_ids) {
        (Some(symbol_id), Some(valid)) if valid.contains(symbol_id) => Some(symbol_id.to_string()),
        (Some(symbol_id), Some(_)) => {
            debug!(
                "Normalizing literal {} containing_symbol_id={} to NULL (missing symbol)",
                literal_id, symbol_id
            );
            None
        }
        (Some(symbol_id), None) => Some(symbol_id.to_string()),
        (None, _) => None,
    }
}

impl SymbolDatabase {
    pub fn bulk_store_literals(&mut self, literals: &[Literal], workspace_id: &str) -> Result<()> {
        if literals.is_empty() {
            return Ok(());
        }

        info!(
            "Starting bulk insert of {} literals with workspace_id: {}",
            literals.len(),
            workspace_id
        );
        let tx = self.conn.transaction()?;
        insert_literals_tx(&tx, literals, None)?;
        tx.commit()?;
        Ok(())
    }
}
