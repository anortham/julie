use std::collections::HashSet;

use anyhow::Result;
use rusqlite::{Transaction, params};
use tracing::{debug, info};

use crate::database::SymbolDatabase;
use crate::extractors::Relationship;

pub(crate) fn insert_relationships_tx(
    tx: &Transaction<'_>,
    relationships: &[Relationship],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    if relationships.is_empty() {
        return Ok(0);
    }

    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO relationships
         (id, from_symbol_id, to_symbol_id, kind, file_path, line_number, confidence, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;

    let mut inserted = 0;
    for rel in relationships {
        if let Some(valid) = valid_symbol_ids {
            if !valid.contains(&rel.from_symbol_id) || !valid.contains(&rel.to_symbol_id) {
                debug!(
                    "Skipping relationship {} -> {} (missing symbol reference)",
                    rel.from_symbol_id, rel.to_symbol_id
                );
                continue;
            }
        }

        let metadata_json = rel
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        stmt.execute(params![
            rel.id,
            rel.from_symbol_id,
            rel.to_symbol_id,
            rel.kind.to_string(),
            rel.file_path,
            rel.line_number,
            rel.confidence,
            metadata_json
        ])?;
        inserted += 1;
    }

    Ok(inserted)
}

impl SymbolDatabase {
    pub fn store_relationships(&mut self, relationships: &[Relationship]) -> Result<()> {
        self.bulk_store_relationships(relationships)
    }

    pub fn bulk_store_relationships(&mut self, relationships: &[Relationship]) -> Result<()> {
        if relationships.is_empty() {
            return Ok(());
        }

        info!(
            "Starting bulk insert of {} relationships",
            relationships.len()
        );
        let tx = self.conn.transaction()?;
        insert_relationships_tx(&tx, relationships, None)?;
        tx.commit()?;
        Ok(())
    }
}
