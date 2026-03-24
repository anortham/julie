//! SQL query engine and output formatting for metrics queries.
//!
//! Queries the symbols table with ORDER BY on analysis-derived fields
//! (security_risk, change_risk, test_coverage, reference_score) stored
//! in the metadata JSON blob and the reference_score column.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::database::SymbolDatabase;
use crate::tools::search::matches_glob_pattern;

/// A single metrics query result row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResult {
    pub name: String,
    pub file_path: String,
    pub start_line: u32,
    pub kind: String,
    pub reference_score: f64,
    pub security_risk_score: Option<f64>,
    pub security_risk_label: Option<String>,
    pub change_risk_score: Option<f64>,
    pub change_risk_label: Option<String>,
    pub test_coverage_tier: Option<String>,
    pub test_count: Option<u32>,
    pub raw_metadata: Option<serde_json::Value>,
}

/// Query symbols ordered by the requested metric.
///
/// Builds a SQL query against the symbols table, using `json_extract` for
/// metadata-derived fields and `reference_score` for centrality.
/// Post-filters by `file_pattern` using glob matching.
pub fn query_by_metrics(
    db: &SymbolDatabase,
    sort_by: &str,
    order: &str,
    min_risk: Option<&str>,
    has_tests: Option<bool>,
    kind: Option<&str>,
    file_pattern: Option<&str>,
    language: Option<&str>,
    exclude_tests: bool,
    limit: u32,
) -> Result<Vec<MetricsResult>> {
    // Build ORDER BY clause based on sort_by field
    let order_dir = if order.eq_ignore_ascii_case("asc") {
        "ASC"
    } else {
        "DESC"
    };

    let order_clause = match sort_by {
        "security_risk" => {
            format!("COALESCE(json_extract(metadata, '$.security_risk.score'), 0.0) {order_dir}")
        }
        "change_risk" => {
            format!("COALESCE(json_extract(metadata, '$.change_risk.score'), 0.0) {order_dir}")
        }
        "centrality" => format!("reference_score {order_dir}"),
        "test_coverage" => {
            format!("COALESCE(json_extract(metadata, '$.test_coverage.test_count'), 0) {order_dir}")
        }
        _ => format!("COALESCE(json_extract(metadata, '$.security_risk.score'), 0.0) {order_dir}"),
    };

    // Build WHERE clauses
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    // Exclude imports/exports (never interesting for metrics)
    conditions.push("kind NOT IN ('import', 'export')".to_string());

    // Exclude test symbols by metadata flag
    if exclude_tests {
        conditions.push(
            "(json_extract(metadata, '$.is_test') IS NULL OR json_extract(metadata, '$.is_test') != 1)"
                .to_string(),
        );
    }

    // min_risk filter (applies to the sort_by field's risk label)
    if let Some(min_risk) = min_risk {
        let risk_path = match sort_by {
            "change_risk" => "$.change_risk.label",
            _ => "$.security_risk.label",
        };
        match min_risk.to_uppercase().as_str() {
            "HIGH" => {
                conditions.push(format!("json_extract(metadata, '{risk_path}') = 'HIGH'"));
            }
            "MEDIUM" => {
                conditions.push(format!(
                    "json_extract(metadata, '{risk_path}') IN ('HIGH', 'MEDIUM')"
                ));
            }
            "LOW" => {
                conditions.push(format!(
                    "json_extract(metadata, '{risk_path}') IN ('HIGH', 'MEDIUM', 'LOW')"
                ));
            }
            _ => {}
        }
    }

    // has_tests filter
    if let Some(has_tests) = has_tests {
        if has_tests {
            // Only symbols WITH test coverage
            conditions.push("json_extract(metadata, '$.test_coverage.test_count') > 0".to_string());
        } else {
            // Only symbols WITHOUT test coverage
            conditions.push(
                "(json_extract(metadata, '$.test_coverage.test_count') IS NULL OR json_extract(metadata, '$.test_coverage.test_count') = 0)"
                    .to_string(),
            );
        }
    }

    // kind filter
    if let Some(kind) = kind {
        conditions.push(format!("kind = ?{}", params.len() + 1));
        params.push(Box::new(kind.to_string()));
    }

    // language filter
    if let Some(language) = language {
        conditions.push(format!("language = ?{}", params.len() + 1));
        params.push(Box::new(language.to_string()));
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // When file_pattern is set, omit the SQL LIMIT so post-filtering can reach the
    // user's requested count even for narrow globs. A fixed multiplier (e.g. 5x)
    // fails when the glob matches only a small fraction of the total rows.
    let use_sql_limit = file_pattern.is_none();

    let sql = format!(
        "SELECT name, file_path, COALESCE(start_line, 0), kind, reference_score, metadata
         FROM symbols
         {where_clause}
         ORDER BY {order_clause}{}",
        if use_sql_limit {
            format!(" LIMIT ?{}", params.len() + 1)
        } else {
            String::new()
        }
    );

    debug!("Metrics query SQL: {}", sql);

    if use_sql_limit {
        params.push(Box::new(limit));
    }

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = db.conn.prepare(&sql)?;
    let rows = stmt.query_map(param_refs.as_slice(), |row| {
        let name: String = row.get(0)?;
        let file_path: String = row.get(1)?;
        let start_line: u32 = row.get(2)?;
        let kind: String = row.get(3)?;
        let reference_score: f64 = row.get(4)?;
        let metadata_str: Option<String> = row.get(5)?;
        Ok((
            name,
            file_path,
            start_line,
            kind,
            reference_score,
            metadata_str,
        ))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (name, file_path, start_line, kind, reference_score, metadata_str) = row?;

        // Post-filter by file_pattern (glob match)
        if let Some(pattern) = file_pattern {
            if !matches_glob_pattern(&file_path, pattern) {
                continue;
            }
        }

        // Parse metadata JSON
        let raw_metadata = metadata_str
            .as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());

        let security_risk_score = raw_metadata
            .as_ref()
            .and_then(|v| v.get("security_risk"))
            .and_then(|v| v.get("score"))
            .and_then(|v| v.as_f64());

        let security_risk_label = raw_metadata
            .as_ref()
            .and_then(|v| v.get("security_risk"))
            .and_then(|v| v.get("label"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let change_risk_score = raw_metadata
            .as_ref()
            .and_then(|v| v.get("change_risk"))
            .and_then(|v| v.get("score"))
            .and_then(|v| v.as_f64());

        let change_risk_label = raw_metadata
            .as_ref()
            .and_then(|v| v.get("change_risk"))
            .and_then(|v| v.get("label"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let test_coverage_tier = raw_metadata
            .as_ref()
            .and_then(|v| v.get("test_coverage"))
            .and_then(|v| v.get("best_tier"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let test_count = raw_metadata
            .as_ref()
            .and_then(|v| v.get("test_coverage"))
            .and_then(|v| v.get("test_count"))
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        results.push(MetricsResult {
            name,
            file_path,
            start_line,
            kind,
            reference_score,
            security_risk_score,
            security_risk_label,
            change_risk_score,
            change_risk_label,
            test_coverage_tier,
            test_count,
            raw_metadata,
        });

        if results.len() >= limit as usize {
            break;
        }
    }

    debug!("Metrics query returned {} results", results.len());
    Ok(results)
}

/// Format metrics results into a human-readable string.
pub fn format_metrics_output(results: &[MetricsResult], sort_by: &str, order: &str) -> String {
    if results.is_empty() {
        return "No symbols match the query filters.".to_string();
    }

    let mut output = String::new();
    output.push_str(&format!(
        "Metrics: {} results (sorted by {} {})\n\n",
        results.len(),
        sort_by,
        order.to_uppercase()
    ));

    for (i, r) in results.iter().enumerate() {
        output.push_str(&format!("{}. {} [{}]\n", i + 1, r.name, r.kind));
        output.push_str(&format!("   {}:{}\n", r.file_path, r.start_line));

        // Security risk
        if let (Some(score), Some(label)) = (r.security_risk_score, &r.security_risk_label) {
            output.push_str(&format!("   Security: {} ({:.2})\n", label, score));
        }

        // Change risk
        if let (Some(score), Some(label)) = (r.change_risk_score, &r.change_risk_label) {
            output.push_str(&format!("   Change risk: {} ({:.2})\n", label, score));
        }

        // Test coverage
        match (&r.test_coverage_tier, r.test_count) {
            (Some(tier), Some(count)) => {
                output.push_str(&format!("   Tests: {} ({} tests)\n", tier, count));
            }
            _ => {
                output.push_str("   Tests: untested\n");
            }
        }

        // Centrality
        output.push_str(&format!("   Centrality: {:.1}\n", r.reference_score));

        if i < results.len() - 1 {
            output.push('\n');
        }
    }

    output
}
