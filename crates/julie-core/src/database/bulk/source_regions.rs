use std::collections::HashSet;

use anyhow::Result;
use julie_extractors::base::SourceRegion;
use rusqlite::{Transaction, params};

pub(crate) fn insert_source_regions_tx(
    tx: &Transaction<'_>,
    regions: &[SourceRegion],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO source_regions
         (id, file_path, language, kind, containing_symbol_id, start_line, start_col,
          end_line, end_col, start_byte, end_byte, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for region in regions {
        let containing_symbol_id = region
            .containing_symbol_id
            .as_deref()
            .filter(|id| valid_symbol_ids.is_none_or(|valid| valid.contains(*id)));
        let metadata = region
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        stmt.execute(params![
            region.id,
            region.file_path,
            region.language,
            region.kind.as_str(),
            containing_symbol_id,
            region.start_line,
            region.start_column,
            region.end_line,
            region.end_column,
            region.start_byte,
            region.end_byte,
            metadata,
        ])?;
    }
    Ok(regions.len() as i64)
}
