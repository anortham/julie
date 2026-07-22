use std::collections::BTreeMap;

use anyhow::Result;
use julie_extractors::base::StructuralFact;
use rusqlite::params_from_iter;
use rusqlite::types::Value;

use super::SymbolDatabase;

#[derive(Debug, Clone)]
pub struct StructuralFactQuery {
    pub pattern_ids: Vec<String>,
    pub path_pattern: Option<String>,
    pub language: Option<String>,
    pub metadata_equals: Vec<(String, String)>,
    pub limit: usize,
}

impl Default for StructuralFactQuery {
    fn default() -> Self {
        Self {
            pattern_ids: Vec::new(),
            path_pattern: None,
            language: None,
            metadata_equals: Vec::new(),
            limit: 50,
        }
    }
}

impl SymbolDatabase {
    pub fn observed_structural_patterns(
        &self,
        language: Option<&str>,
        path_pattern: Option<&str>,
    ) -> Result<Vec<(String, u64)>> {
        let mut sql = String::from("SELECT pattern_id, file_path FROM structural_facts");
        let mut values = Vec::new();
        if let Some(language) = language {
            sql.push_str(" WHERE language = ?");
            values.push(Value::Text(language.to_string()));
        }
        sql.push_str(" ORDER BY pattern_id, file_path LIMIT 10000");

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(values), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut counts = BTreeMap::<String, u64>::new();
        for (pattern_id, file_path) in rows {
            if path_pattern
                .is_some_and(|pattern| !crate::glob::matches_glob_pattern(&file_path, pattern))
            {
                continue;
            }
            *counts.entry(pattern_id).or_default() += 1;
        }
        let mut observed = counts.into_iter().collect::<Vec<_>>();
        observed.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        Ok(observed)
    }

    pub fn search_structural_facts(
        &self,
        query: &StructuralFactQuery,
    ) -> Result<Vec<StructuralFact>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }

        let mut sql = String::from(
            "SELECT id, file_path, language, pattern_id, capture_name, node_kind,
                    containing_symbol_id, start_line, start_col, end_line, end_col,
                    start_byte, end_byte, confidence, metadata
             FROM structural_facts",
        );
        let mut clauses = Vec::new();
        let mut values = Vec::new();
        if !query.pattern_ids.is_empty() {
            clauses.push(format!(
                "pattern_id IN ({})",
                vec!["?"; query.pattern_ids.len()].join(", ")
            ));
            values.extend(query.pattern_ids.iter().cloned().map(Value::Text));
        }
        if let Some(language) = &query.language {
            clauses.push("language = ?".to_string());
            values.push(Value::Text(language.clone()));
        }
        for (key, value) in &query.metadata_equals {
            clauses.push("json_valid(metadata) AND json_extract(metadata, ?) = ?".to_string());
            values.push(Value::Text(format!("$.\"{}\"", key.replace('"', "\\\""))));
            values.push(Value::Text(value.clone()));
        }
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY pattern_id, file_path, start_byte, id LIMIT ?");
        let database_limit = query.limit.saturating_mul(10).clamp(100, 5000);
        values.push(Value::Integer(database_limit as i64));

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(values), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, u32>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, u32>(9)?,
                    row.get::<_, u32>(10)?,
                    row.get::<_, u32>(11)?,
                    row.get::<_, u32>(12)?,
                    row.get::<_, f32>(13)?,
                    row.get::<_, Option<String>>(14)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        rows.into_iter()
            .filter(|row| {
                query
                    .path_pattern
                    .as_deref()
                    .is_none_or(|pattern| crate::glob::matches_glob_pattern(&row.1, pattern))
            })
            .take(query.limit)
            .map(
                |(
                    id,
                    file_path,
                    language,
                    pattern_id,
                    capture_name,
                    node_kind,
                    containing_symbol_id,
                    start_line,
                    start_column,
                    end_line,
                    end_column,
                    start_byte,
                    end_byte,
                    confidence,
                    metadata,
                )| {
                    let metadata = metadata
                        .map(|value| serde_json::from_str(&value))
                        .transpose()?;
                    Ok(StructuralFact {
                        id,
                        file_path,
                        language,
                        pattern_id,
                        capture_name,
                        node_kind,
                        containing_symbol_id,
                        start_line,
                        start_column,
                        end_line,
                        end_column,
                        start_byte,
                        end_byte,
                        confidence,
                        metadata,
                    })
                },
            )
            .collect()
    }

    /// Load *all* structural facts matching any of `pattern_ids`, with no
    /// artificial row cap. Used by the web-edge rebuild pass, which must see
    /// every client-call / route-handler fact in the workspace to compute the
    /// cross-file join (the capped `search_structural_facts` would silently
    /// drop edges beyond 5000 rows).
    pub fn load_all_structural_facts_by_pattern_ids(
        &self,
        pattern_ids: &[&str],
    ) -> Result<Vec<StructuralFact>> {
        if pattern_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; pattern_ids.len()].join(", ");
        let sql = format!(
            "SELECT id, file_path, language, pattern_id, capture_name, node_kind,
                    containing_symbol_id, start_line, start_col, end_line, end_col,
                    start_byte, end_byte, confidence, metadata
             FROM structural_facts
             WHERE pattern_id IN ({placeholders})
             ORDER BY pattern_id, file_path, start_byte, id"
        );
        let params: Vec<Value> = pattern_ids
            .iter()
            .map(|p| Value::Text((*p).to_string()))
            .collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt
            .query_map(params_from_iter(params), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, u32>(7)?,
                    row.get::<_, u32>(8)?,
                    row.get::<_, u32>(9)?,
                    row.get::<_, u32>(10)?,
                    row.get::<_, u32>(11)?,
                    row.get::<_, u32>(12)?,
                    row.get::<_, f32>(13)?,
                    row.get::<_, Option<String>>(14)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(
                |(
                    id,
                    file_path,
                    language,
                    pattern_id,
                    capture_name,
                    node_kind,
                    containing_symbol_id,
                    start_line,
                    start_column,
                    end_line,
                    end_column,
                    start_byte,
                    end_byte,
                    confidence,
                    metadata,
                )| {
                    let metadata = metadata
                        .map(|value| serde_json::from_str(&value))
                        .transpose()?;
                    Ok(StructuralFact {
                        id,
                        file_path,
                        language,
                        pattern_id,
                        capture_name,
                        node_kind,
                        containing_symbol_id,
                        start_line,
                        start_column,
                        end_line,
                        end_column,
                        start_byte,
                        end_byte,
                        confidence,
                        metadata,
                    })
                },
            )
            .collect()
    }

    /// Load all structural facts whose `containing_symbol_id` is in
    /// `symbol_ids` (non-null). Used by the search projection pass to index
    /// web/route/SQL fact tokens on their containing symbol. Ordered by
    /// containing symbol, then start byte, then id for deterministic output.
    pub fn load_structural_facts_for_symbols(
        &self,
        symbol_ids: &[String],
    ) -> Result<Vec<StructuralFact>> {
        if symbol_ids.is_empty() {
            return Ok(Vec::new());
        }

        const CHUNK_SIZE: usize = 500;
        let mut unique_ids = symbol_ids.to_vec();
        unique_ids.sort();
        unique_ids.dedup();
        let mut facts = Vec::new();

        for chunk in unique_ids.chunks(CHUNK_SIZE) {
            let placeholders = vec!["?"; chunk.len()].join(", ");
            let sql = format!(
                "SELECT id, file_path, language, pattern_id, capture_name, node_kind,
                        containing_symbol_id, start_line, start_col, end_line, end_col,
                        start_byte, end_byte, confidence, metadata
                 FROM structural_facts
                 WHERE containing_symbol_id IN ({placeholders})
                 ORDER BY containing_symbol_id, start_byte, id"
            );
            let params: Vec<Value> = chunk
                .iter()
                .map(|symbol_id| Value::Text(symbol_id.clone()))
                .collect();
            let mut stmt = self.conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(params), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, u32>(7)?,
                        row.get::<_, u32>(8)?,
                        row.get::<_, u32>(9)?,
                        row.get::<_, u32>(10)?,
                        row.get::<_, u32>(11)?,
                        row.get::<_, u32>(12)?,
                        row.get::<_, f32>(13)?,
                        row.get::<_, Option<String>>(14)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            let chunk_facts = rows
                .into_iter()
                .map(
                    |(
                        id,
                        file_path,
                        language,
                        pattern_id,
                        capture_name,
                        node_kind,
                        containing_symbol_id,
                        start_line,
                        start_column,
                        end_line,
                        end_column,
                        start_byte,
                        end_byte,
                        confidence,
                        metadata,
                    )| {
                        let metadata = metadata
                            .map(|value| serde_json::from_str(&value))
                            .transpose()?;
                        Ok(StructuralFact {
                            id,
                            file_path,
                            language,
                            pattern_id,
                            capture_name,
                            node_kind,
                            containing_symbol_id,
                            start_line,
                            start_column,
                            end_line,
                            end_column,
                            start_byte,
                            end_byte,
                            confidence,
                            metadata,
                        })
                    },
                )
                .collect::<Result<Vec<_>>>()?;
            facts.extend(chunk_facts);
        }

        facts.sort_by(|left, right| {
            left.containing_symbol_id
                .cmp(&right.containing_symbol_id)
                .then_with(|| left.start_byte.cmp(&right.start_byte))
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(facts)
    }
}
