//! Metrics queries over the snapshot graph.

use anyhow::Result;
use julie_extractors::SymbolKind;
use julie_index::graph::{Graph, SymbolId};
use julie_index::search::scoring::is_test_path;
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::analysis::test_linkage::test_linkage_entry;
use crate::tools::search::matches_glob_pattern;

/// A single metrics query result row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsResult {
    pub name: String,
    pub file_path: String,
    pub start_line: u32,
    pub kind: String,
    pub reference_score: f64,
    pub change_risk_score: Option<f64>,
    pub change_risk_label: Option<String>,
    pub test_linkage_tier: Option<String>,
    pub test_count: Option<u32>,
    pub raw_metadata: Option<serde_json::Value>,
}

/// Query symbols ordered by the requested metric.
pub fn query_by_metrics(
    graph: &Graph,
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
    let descending = !order.eq_ignore_ascii_case("asc");
    let mut results = Vec::new();
    for index in 0..graph.len() {
        let id = SymbolId(index as u32);
        let row = graph.symbol(id);
        if matches!(row.kind, SymbolKind::Import | SymbolKind::Export) {
            continue;
        }
        let raw_metadata = row
            .metadata
            .as_ref()
            .map(|metadata| serde_json::to_value(metadata).unwrap_or(serde_json::Value::Null));
        if exclude_tests {
            let is_test = raw_metadata
                .as_ref()
                .and_then(|value| value.get("is_test"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
                || is_test_path(&row.path);
            if is_test {
                continue;
            }
        }
        if let Some(kind) = kind {
            let kind_name = format!("{:?}", row.kind).to_lowercase();
            if kind_name != kind.to_lowercase() {
                continue;
            }
        }
        if let Some(language) = language
            && row.language != language
        {
            continue;
        }
        if let Some(pattern) = file_pattern
            && !matches_glob_pattern(&row.path, pattern)
        {
            continue;
        }

        let change_risk_score = raw_metadata
            .as_ref()
            .and_then(|value| value.get("change_risk"))
            .and_then(|value| value.get("score"))
            .and_then(|value| value.as_f64());
        let change_risk_label = raw_metadata
            .as_ref()
            .and_then(|value| value.get("change_risk"))
            .and_then(|value| value.get("label"))
            .and_then(|value| value.as_str())
            .map(str::to_string);
        if let Some(min_risk) = min_risk {
            let rank = match change_risk_label.as_deref() {
                Some("HIGH") => 3,
                Some("MEDIUM") => 2,
                Some("LOW") => 1,
                _ => 0,
            };
            let needed = match min_risk.to_uppercase().as_str() {
                "HIGH" => 3,
                "MEDIUM" => 2,
                "LOW" => 1,
                _ => 0,
            };
            if rank < needed {
                continue;
            }
        }

        let test_linkage = raw_metadata.as_ref().and_then(test_linkage_entry);
        let test_count = test_linkage
            .and_then(|value| value.get("test_count"))
            .and_then(|value| value.as_u64())
            .map(|n| n as u32);
        if let Some(has_tests) = has_tests {
            let linked = test_count.unwrap_or(0) > 0;
            if has_tests != linked {
                continue;
            }
        }

        results.push(MetricsResult {
            name: row.name.clone(),
            file_path: row.path.clone(),
            start_line: row.span.start_line,
            kind: format!("{:?}", row.kind).to_lowercase(),
            reference_score: graph.reference_score(id),
            change_risk_score,
            change_risk_label,
            test_linkage_tier: test_linkage
                .and_then(|value| value.get("best_tier"))
                .and_then(|value| value.as_str())
                .map(str::to_string),
            test_count,
            raw_metadata,
        });
    }

    results.sort_by(|left, right| {
        let cmp = match sort_by {
            "change_risk" => left
                .change_risk_score
                .unwrap_or(0.0)
                .partial_cmp(&right.change_risk_score.unwrap_or(0.0)),
            "test_linkage" | "test_coverage" => left
                .test_count
                .unwrap_or(0)
                .partial_cmp(&right.test_count.unwrap_or(0)),
            _ => left.reference_score.partial_cmp(&right.reference_score),
        }
        .unwrap_or(std::cmp::Ordering::Equal);
        if descending { cmp.reverse() } else { cmp }
    });
    results.truncate(limit as usize);
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

        // Change risk
        if let (Some(score), Some(label)) = (r.change_risk_score, &r.change_risk_label) {
            output.push_str(&format!("   Change risk: {} ({:.2})\n", label, score));
        }

        match (&r.test_linkage_tier, r.test_count) {
            (Some(tier), Some(count)) => {
                output.push_str(&format!("   Linked tests: {} ({} tests)\n", tier, count));
            }
            _ => {
                output.push_str("   Linked tests: none\n");
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
