use std::collections::HashSet;

use anyhow::Result;
use rusqlite::{Transaction, params};
use tracing::{debug, info};

use crate::database::SymbolDatabase;

pub(crate) fn insert_types_tx(
    tx: &Transaction<'_>,
    types: &[julie_extractors::base::TypeInfo],
    valid_symbol_ids: Option<&HashSet<String>>,
    now: i64,
) -> Result<i64> {
    if types.is_empty() {
        return Ok(0);
    }

    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO types
         (symbol_id, resolved_type, generic_params, constraints, is_inferred, language, metadata, last_indexed)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;

    let mut inserted = 0;
    for type_info in types {
        if let Some(valid) = valid_symbol_ids {
            if !valid.contains(&type_info.symbol_id) {
                debug!(
                    "Skipping type row for missing symbol reference {}",
                    type_info.symbol_id
                );
                continue;
            }
        }

        let generic_params_json = type_info
            .generic_params
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let constraints_json = type_info
            .constraints
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let metadata_json = type_info
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        stmt.execute(params![
            type_info.symbol_id,
            type_info.resolved_type,
            generic_params_json,
            constraints_json,
            type_info.is_inferred,
            type_info.language,
            metadata_json,
            now
        ])?;
        inserted += 1;
    }

    Ok(inserted)
}

impl SymbolDatabase {
    pub fn bulk_store_types(
        &mut self,
        types: &[julie_extractors::base::TypeInfo],
        _workspace_id: &str,
    ) -> Result<()> {
        if types.is_empty() {
            return Ok(());
        }

        info!("Starting bulk insert of {} types", types.len());
        let tx = self.conn.transaction()?;
        insert_types_tx(&tx, types, None, unix_timestamp()?)?;
        tx.commit()?;
        Ok(())
    }
}

fn unix_timestamp() -> Result<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64)
}
