use std::collections::HashSet;

use anyhow::Result;
use julie_extractors::base::ComplexityMetric;
use rusqlite::{Transaction, params};

pub(crate) fn insert_complexity_metrics_tx(
    tx: &Transaction<'_>,
    metrics: &[ComplexityMetric],
    valid_symbol_ids: Option<&HashSet<String>>,
) -> Result<i64> {
    let mut stmt = tx.prepare(
        "INSERT OR REPLACE INTO complexity_metrics
         (id, file_path, language, scope, symbol_id, algorithm_id, covered_lines,
          covered_bytes, decision_count, loop_count, max_nesting_depth,
          parameter_count, start_line, start_col, end_line, end_col, start_byte,
          end_byte, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                 ?14, ?15, ?16, ?17, ?18, ?19)",
    )?;
    for metric in metrics {
        let symbol_id = metric
            .symbol_id
            .as_deref()
            .filter(|id| valid_symbol_ids.is_none_or(|valid| valid.contains(*id)));
        let metadata = metric
            .metadata
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        stmt.execute(params![
            metric.id,
            metric.file_path,
            metric.language,
            metric.scope,
            symbol_id,
            metric.algorithm_id,
            metric.covered_lines,
            metric.covered_bytes,
            metric.decision_count,
            metric.loop_count,
            metric.max_nesting_depth,
            metric.parameter_count,
            metric.start_line,
            metric.start_column,
            metric.end_line,
            metric.end_column,
            metric.start_byte,
            metric.end_byte,
            metadata,
        ])?;
    }
    Ok(metrics.len() as i64)
}
