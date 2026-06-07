use std::collections::HashSet;

use anyhow::Result;
use rusqlite::{Transaction, params};
use tracing::{debug, info};

use crate::database::SymbolDatabase;

const IDENTIFIER_SECONDARY_INDEXES: &[(&str, &str)] = &[
    ("idx_identifiers_name", "identifiers(name)"),
    ("idx_identifiers_file", "identifiers(file_path)"),
    (
        "idx_identifiers_containing",
        "identifiers(containing_symbol_id)",
    ),
    ("idx_identifiers_target", "identifiers(target_symbol_id)"),
    ("idx_identifiers_kind", "identifiers(kind)"),
    (
        "idx_identifiers_file_line_kind",
        "identifiers(file_path, start_line, kind)",
    ),
    ("idx_identifiers_file_name", "identifiers(file_path, name)"),
    (
        "idx_identifiers_kind_containing",
        "identifiers(kind, containing_symbol_id)",
    ),
    (
        "idx_identifiers_name_kind_containing",
        "identifiers(name, kind, containing_symbol_id)",
    ),
];

pub(crate) fn insert_identifiers_tx(
    tx: &Transaction<'_>,
    identifiers: &[julie_extractors::Identifier],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    if identifiers.is_empty() {
        return Ok(0);
    }

    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO identifiers
         (id, name, kind, language, file_path, start_line, start_col,
          end_line, end_col, start_byte, end_byte, containing_symbol_id,
          target_symbol_id, confidence, code_context)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
    )?;

    let mut inserted = 0;
    for identifier in identifiers {
        let containing_symbol_id = normalize_symbol_ref(
            identifier.containing_symbol_id.as_deref(),
            valid_symbol_ids,
            &identifier.id,
            "containing_symbol_id",
        );
        let target_symbol_id = normalize_symbol_ref(
            identifier.target_symbol_id.as_deref(),
            valid_symbol_ids,
            &identifier.id,
            "target_symbol_id",
        );

        stmt.execute(params![
            identifier.id,
            identifier.name,
            identifier.kind.to_string(),
            identifier.language,
            identifier.file_path,
            identifier.start_line,
            identifier.start_column,
            identifier.end_line,
            identifier.end_column,
            identifier.start_byte,
            identifier.end_byte,
            containing_symbol_id,
            target_symbol_id,
            identifier.confidence,
            identifier.code_context
        ])?;
        inserted += 1;
    }

    Ok(inserted)
}

pub(crate) fn insert_identifiers_with_deferred_indexes_tx(
    tx: &Transaction<'_>,
    identifiers: &[julie_extractors::Identifier],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    if identifiers.is_empty() {
        return Ok(0);
    }

    drop_identifier_secondary_indexes_tx(tx)?;
    let inserted = insert_identifiers_tx(tx, identifiers, valid_symbol_ids)?;
    create_identifier_secondary_indexes_tx(tx)?;
    Ok(inserted)
}

fn drop_identifier_secondary_indexes_tx(tx: &Transaction<'_>) -> Result<()> {
    for (name, _) in IDENTIFIER_SECONDARY_INDEXES {
        tx.execute(&format!("DROP INDEX IF EXISTS {name}"), [])?;
    }
    Ok(())
}

fn create_identifier_secondary_indexes_tx(tx: &Transaction<'_>) -> Result<()> {
    for (name, definition) in IDENTIFIER_SECONDARY_INDEXES {
        tx.execute(
            &format!("CREATE INDEX IF NOT EXISTS {name} ON {definition}"),
            [],
        )?;
    }
    Ok(())
}

fn normalize_symbol_ref(
    symbol_id: Option<&str>,
    valid_symbol_ids: Option<&HashSet<String>>,
    identifier_id: &str,
    field: &str,
) -> Option<String> {
    match (symbol_id, valid_symbol_ids) {
        (Some(symbol_id), Some(valid)) if valid.contains(symbol_id) => Some(symbol_id.to_string()),
        (Some(symbol_id), Some(_)) => {
            debug!(
                "Normalizing identifier {} {}={} to NULL (missing symbol)",
                identifier_id, field, symbol_id
            );
            None
        }
        (Some(symbol_id), None) => Some(symbol_id.to_string()),
        (None, _) => None,
    }
}

impl SymbolDatabase {
    pub fn bulk_store_identifiers(
        &mut self,
        identifiers: &[julie_extractors::Identifier],
        workspace_id: &str,
    ) -> Result<()> {
        if identifiers.is_empty() {
            return Ok(());
        }

        info!(
            "Starting bulk insert of {} identifiers with workspace_id: {}",
            identifiers.len(),
            workspace_id
        );
        let tx = self.conn.transaction()?;
        insert_identifiers_tx(&tx, identifiers, None)?;
        tx.commit()?;
        Ok(())
    }
}
