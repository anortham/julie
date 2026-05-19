use std::collections::HashMap;

use anyhow::{Result, anyhow};

use crate::database::SymbolDatabase;
use crate::indexing_core::batch::ExtractedBatch;
use crate::indexing_core::extraction::{ExtractedFileDisposition, ExtractedFileRecord};

pub(crate) fn ensure_batch_preserves_known_good_symbols(
    db: &SymbolDatabase,
    batch: &ExtractedBatch,
    records: &[ExtractedFileRecord],
) -> Result<()> {
    let mut new_symbol_counts: HashMap<&str, usize> = HashMap::new();
    for symbol in &batch.all_symbols {
        *new_symbol_counts
            .entry(symbol.file_path.as_str())
            .or_default() += 1;
    }

    for record in records {
        let requires_guard = matches!(
            record.disposition,
            ExtractedFileDisposition::Parsed | ExtractedFileDisposition::RepairNeeded { .. }
        );
        if !requires_guard {
            continue;
        }
        if new_symbol_counts
            .get(record.relative_path.as_str())
            .copied()
            .unwrap_or(0)
            > 0
        {
            continue;
        }
        let existing_symbols = existing_symbol_count(db, &record.relative_path)?;
        if existing_symbols == 0 {
            continue;
        }
        let detail = match &record.disposition {
            ExtractedFileDisposition::RepairNeeded { detail } => {
                format!("parser failed: {detail}")
            }
            ExtractedFileDisposition::Parsed => "parser returned zero symbols".to_string(),
            ExtractedFileDisposition::TextOnly => unreachable!("text-only records are skipped"),
        };
        return Err(anyhow!(
            "extraction for '{}' would remove existing symbols ({existing_symbols}); {detail}",
            record.relative_path
        ));
    }

    Ok(())
}

fn existing_symbol_count(db: &SymbolDatabase, relative_path: &str) -> Result<i64> {
    Ok(db.conn.query_row(
        "SELECT COUNT(*) FROM symbols WHERE file_path = ?1",
        [relative_path],
        |row| row.get(0),
    )?)
}
