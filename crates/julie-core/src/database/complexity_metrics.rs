use anyhow::Result;
use julie_extractors::base::ComplexityMetric;
use rusqlite::{OptionalExtension, params};

use super::SymbolDatabase;

impl SymbolDatabase {
    pub fn get_complexity_metric_for_symbol(
        &self,
        symbol_id: &str,
    ) -> Result<Option<ComplexityMetric>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, file_path, language, scope, symbol_id, algorithm_id,
                        covered_lines, covered_bytes, decision_count, loop_count,
                        max_nesting_depth, parameter_count, start_line, start_col,
                        end_line, end_col, start_byte, end_byte, metadata
                 FROM complexity_metrics
                 WHERE symbol_id = ?1
                 ORDER BY id
                 LIMIT 1",
                params![symbol_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, u32>(6)?,
                        row.get::<_, u32>(7)?,
                        row.get::<_, u32>(8)?,
                        row.get::<_, u32>(9)?,
                        row.get::<_, u32>(10)?,
                        row.get::<_, Option<u32>>(11)?,
                        row.get::<_, u32>(12)?,
                        row.get::<_, u32>(13)?,
                        row.get::<_, u32>(14)?,
                        row.get::<_, u32>(15)?,
                        row.get::<_, u32>(16)?,
                        row.get::<_, u32>(17)?,
                        row.get::<_, Option<String>>(18)?,
                    ))
                },
            )
            .optional()?;

        row.map(
            |(
                id,
                file_path,
                language,
                scope,
                symbol_id,
                algorithm_id,
                covered_lines,
                covered_bytes,
                decision_count,
                loop_count,
                max_nesting_depth,
                parameter_count,
                start_line,
                start_column,
                end_line,
                end_column,
                start_byte,
                end_byte,
                metadata,
            )| {
                let metadata = metadata
                    .map(|value| serde_json::from_str(&value))
                    .transpose()?;
                Ok(ComplexityMetric {
                    id,
                    file_path,
                    language,
                    scope,
                    symbol_id,
                    algorithm_id,
                    covered_lines,
                    covered_bytes,
                    decision_count,
                    loop_count,
                    max_nesting_depth,
                    parameter_count,
                    start_line,
                    start_column,
                    end_line,
                    end_column,
                    start_byte,
                    end_byte,
                    metadata,
                })
            },
        )
        .transpose()
    }
}
