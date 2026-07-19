use anyhow::{Result, anyhow};
use julie_extractors::base::{SourceRegion, SourceRegionKind};
use rusqlite::params;

use super::SymbolDatabase;

impl SymbolDatabase {
    pub fn get_source_regions_for_file(
        &self,
        file_path: &str,
        kinds: &[SourceRegionKind],
    ) -> Result<Vec<SourceRegion>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_path, language, kind, containing_symbol_id,
                    start_line, start_col, end_line, end_col, start_byte, end_byte, metadata
             FROM source_regions
             WHERE file_path = ?1
             ORDER BY start_byte, end_byte, id",
        )?;
        let rows = stmt
            .query_map(params![file_path], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, u32>(6)?,
                    row.get::<_, u32>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, u32>(9)?,
                    row.get::<_, u32>(10)?,
                    row.get::<_, Option<String>>(11)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .map(
                |(
                    id,
                    file_path,
                    language,
                    kind,
                    containing_symbol_id,
                    start_line,
                    start_column,
                    end_line,
                    end_column,
                    start_byte,
                    end_byte,
                    metadata,
                )| {
                    let kind = parse_source_region_kind(&kind)?;
                    let metadata = metadata
                        .map(|value| serde_json::from_str(&value))
                        .transpose()?;
                    Ok(SourceRegion {
                        id,
                        file_path,
                        language,
                        kind,
                        containing_symbol_id,
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
            .filter(|result| {
                result.as_ref().map_or(true, |region| {
                    kinds.is_empty() || kinds.contains(&region.kind)
                })
            })
            .collect()
    }
}

fn parse_source_region_kind(value: &str) -> Result<SourceRegionKind> {
    match value {
        "comment" => Ok(SourceRegionKind::Comment),
        "doc_comment" => Ok(SourceRegionKind::DocComment),
        "string_literal" => Ok(SourceRegionKind::StringLiteral),
        "embedded" => Ok(SourceRegionKind::Embedded),
        _ => Err(anyhow!("corrupt source region kind: {value}")),
    }
}
