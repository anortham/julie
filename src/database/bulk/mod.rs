use std::collections::HashSet;

use anyhow::Result;
use rusqlite::Transaction;

use crate::extractors::Relationship;

pub(crate) mod atomic;
pub(crate) mod cleanup;
pub(crate) mod identifiers;
pub(crate) mod relationships;
pub(crate) mod types;

pub(crate) fn collect_referenced_symbol_ids(
    relationships: &[Relationship],
    identifiers: &[crate::extractors::Identifier],
    types: &[crate::extractors::base::TypeInfo],
) -> HashSet<String> {
    let mut ids = HashSet::new();
    for rel in relationships {
        ids.insert(rel.from_symbol_id.clone());
        ids.insert(rel.to_symbol_id.clone());
    }
    for identifier in identifiers {
        if let Some(symbol_id) = &identifier.containing_symbol_id {
            ids.insert(symbol_id.clone());
        }
        if let Some(symbol_id) = &identifier.target_symbol_id {
            ids.insert(symbol_id.clone());
        }
    }
    for type_info in types {
        ids.insert(type_info.symbol_id.clone());
    }
    ids
}

pub(crate) fn load_existing_symbol_ids_tx(
    tx: &Transaction<'_>,
    referenced_ids: &HashSet<String>,
) -> Result<HashSet<String>> {
    if referenced_ids.is_empty() {
        return Ok(HashSet::new());
    }

    const CHUNK_SIZE: usize = 500;
    let ids: Vec<&String> = referenced_ids.iter().collect();
    let mut existing = HashSet::new();
    for chunk in ids.chunks(CHUNK_SIZE) {
        let placeholders = (1..=chunk.len())
            .map(|idx| format!("?{idx}"))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!("SELECT id FROM symbols WHERE id IN ({placeholders})");
        let params = chunk
            .iter()
            .map(|id| *id as &dyn rusqlite::ToSql)
            .collect::<Vec<_>>();

        let mut stmt = tx.prepare(&query)?;
        let rows = stmt.query_map(&params[..], |row| row.get::<_, String>(0))?;
        for row in rows {
            existing.insert(row?);
        }
    }
    Ok(existing)
}
