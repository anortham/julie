use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use julie::daemon::database::DaemonDatabase;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::search_matrix_report::write_seed_report;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixSeedReport {
    pub window_days: u32,
    pub candidates: Vec<SearchMatrixSeedCandidate>,
    pub clusters: Vec<SearchMatrixSeedCluster>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixSeedCandidate {
    pub tool_call_id: i64,
    pub workspace_id: String,
    pub session_id: String,
    pub family: String,
    pub query: String,
    pub normalized_query: String,
    pub search_target: String,
    pub language: Option<String>,
    pub file_pattern: Option<String>,
    pub exclude_tests: Option<bool>,
    pub result_count: Option<u64>,
    pub relaxed: Option<bool>,
    pub zero_hit_reason: Option<String>,
    pub file_pattern_diagnostic: Option<String>,
    pub hint_kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatrixSeedCluster {
    pub family: String,
    pub zero_hit_reason: Option<String>,
    pub file_pattern_diagnostic: Option<String>,
    pub hint_kind: Option<String>,
    pub count: usize,
    pub example_queries: Vec<String>,
}

pub fn mine_search_matrix_seed_report(
    daemon_db_path: &Path,
    days: u32,
    out_path: &Path,
) -> Result<SearchMatrixSeedReport> {
    let daemon_db = DaemonDatabase::open(daemon_db_path)?;
    let rows = daemon_db.list_tool_calls_for_search_analysis(days as i64 * 86_400)?;
    let mut candidates = Vec::new();
    for row in rows.iter().filter(|row| row.tool_name == "fast_search") {
        if let Some(candidate) = candidate_from_row(row)? {
            candidates.push(candidate);
        }
    }
    let clusters = cluster_candidates(&candidates);
    let report = SearchMatrixSeedReport {
        window_days: days,
        candidates,
        clusters,
    };
    write_seed_report(&report, out_path)?;
    Ok(report)
}

fn candidate_from_row(
    row: &julie::daemon::database::SearchToolCallRow,
) -> Result<Option<SearchMatrixSeedCandidate>> {
    let Some(metadata_text) = row.metadata.as_deref() else {
        return Ok(None);
    };
    let metadata: Value = serde_json::from_str(metadata_text)?;
    let query = metadata["query"].as_str().unwrap_or_default().to_string();
    let normalized_query = metadata["normalized_query"]
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| normalize_query(&query));
    let search_target = metadata["search_target"]
        .as_str()
        .unwrap_or("definitions")
        .to_string();
    let language = metadata["language"].as_str().map(ToOwned::to_owned);
    let file_pattern = metadata["file_pattern"].as_str().map(ToOwned::to_owned);
    let exclude_tests = metadata["exclude_tests"].as_bool();
    let trace = &metadata["trace"];

    Ok(Some(SearchMatrixSeedCandidate {
        tool_call_id: row.id,
        workspace_id: row.workspace_id.clone(),
        session_id: row.session_id.clone(),
        family: infer_query_family(
            &query,
            &search_target,
            file_pattern.as_deref(),
            exclude_tests,
        ),
        query,
        normalized_query,
        search_target,
        language,
        file_pattern,
        exclude_tests,
        result_count: trace["result_count"].as_u64(),
        relaxed: trace["relaxed"].as_bool(),
        zero_hit_reason: trace["zero_hit_reason"].as_str().map(ToOwned::to_owned),
        file_pattern_diagnostic: trace["file_pattern_diagnostic"]
            .as_str()
            .map(ToOwned::to_owned),
        hint_kind: trace["hint_kind"].as_str().map(ToOwned::to_owned),
    }))
}

fn cluster_candidates(candidates: &[SearchMatrixSeedCandidate]) -> Vec<SearchMatrixSeedCluster> {
    let mut grouped: BTreeMap<
        (String, Option<String>, Option<String>, Option<String>),
        Vec<String>,
    > = BTreeMap::new();

    for candidate in candidates {
        grouped
            .entry((
                candidate.family.clone(),
                candidate.zero_hit_reason.clone(),
                candidate.file_pattern_diagnostic.clone(),
                candidate.hint_kind.clone(),
            ))
            .or_default()
            .push(candidate.query.clone());
    }

    grouped
        .into_iter()
        .map(
            |((family, zero_hit_reason, file_pattern_diagnostic, hint_kind), queries)| {
                let count = queries.len();
                SearchMatrixSeedCluster {
                    family,
                    zero_hit_reason,
                    file_pattern_diagnostic,
                    hint_kind,
                    count,
                    example_queries: queries.into_iter().take(3).collect(),
                }
            },
        )
        .collect()
}

fn infer_query_family(
    query: &str,
    search_target: &str,
    file_pattern: Option<&str>,
    exclude_tests: Option<bool>,
) -> String {
    if exclude_tests.unwrap_or(false) {
        return "exclude_tests".to_string();
    }
    if file_pattern.is_some() && search_target == "content" {
        if file_pattern.is_some_and(|pattern| pattern.contains('|')) {
            return "alternation_file_pattern".to_string();
        }
        if file_pattern.is_some_and(|pattern| pattern.contains('!')) {
            return "exclusion_file_pattern".to_string();
        }
        return "scoped_content".to_string();
    }
    if query.contains('"') {
        return "quoted_phrase".to_string();
    }
    if query.contains('_') {
        return "snake_case".to_string();
    }
    if query.contains('-') {
        return "hyphenated_token".to_string();
    }
    if query.split_whitespace().count() > 1 {
        return "multi_token".to_string();
    }
    if query.chars().any(|ch| ch.is_uppercase()) && query.chars().any(|ch| ch.is_lowercase()) {
        return "camel_case".to_string();
    }
    "exact_identifier".to_string()
}

fn normalize_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}
