use std::collections::HashMap;

use serde_json::{Value, json};

use crate::tools::search::FastSearchTool;
use crate::tools::search::trace::SearchExecutionResult;

const TRACE_VERSION: &str = "fast_search_trace_v1";

pub(crate) fn fast_search_metadata(
    params: &FastSearchTool,
    execution: Option<&SearchExecutionResult>,
) -> Value {
    let intent = infer_intent(&params.query);
    let trace = execution.map(|result| {
        let kind_distribution = compute_kind_distribution(result);
        json!({
            "strategy": result.trace.strategy_id,
            "returned_hit_count": result.hits.len(),
            "result_count": result.total_results,
            "relaxed": result.relaxed,
            "top_hits": result.trace.top_hits,
            "zero_hit_reason": result.trace.zero_hit_reason,
            "file_pattern_diagnostic": result.trace.file_pattern_diagnostic,
            "hint_kind": result.trace.hint_kind,
            "line_match_strategy": result.trace.line_match_strategy,
            "definition_exact_match": result.trace.definition_exact_match,
            "target_hint": result.trace.target_hint,
            "scope_relaxed": result.trace.scope_relaxed,
            "original_file_pattern": result.trace.original_file_pattern,
            "original_zero_hit_reason": result.trace.original_zero_hit_reason,
            "scope_rescue_count": result.trace.scope_rescue_count,
            "or_disjunction_detected": result.trace.or_disjunction_detected,
            "kind_distribution": kind_distribution,
        })
    });

    json!({
        "query": params.query,
        "normalized_query": normalize_query(&params.query),
        "language": params.language,
        "file_pattern": params.file_pattern,
        "limit": params.effective_limit(),
        "exclude_tests": params.exclude_tests,
        "workspace": params.workspace,
        "intent": intent,
        "trace_version": TRACE_VERSION,
        "trace": trace,
    })
}

fn compute_kind_distribution(result: &SearchExecutionResult) -> HashMap<String, u32> {
    let mut dist: HashMap<String, u32> = HashMap::new();
    for hit in &result.hits {
        *dist.entry(hit.kind.clone()).or_insert(0) += 1;
    }
    dist
}

pub(crate) fn fast_search_source_paths(execution: Option<&SearchExecutionResult>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    execution
        .into_iter()
        .flat_map(|result| result.hits.iter().map(|hit| hit.file.clone()))
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn infer_intent(query: &str) -> &'static str {
    let normalized = query.to_lowercase();
    let token_count = query.split_whitespace().count();
    let has_symbol_shape = query.contains("::")
        || query.contains('_')
        || query.chars().any(|ch| ch.is_ascii_uppercase());
    let has_tool_phrase = [
        "find references",
        "call path",
        "wrapper",
        "handler",
        "mcp",
        "tool",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase));
    let has_grep_shape =
        normalized.contains("todo") || normalized.contains("fixme") || normalized.contains("grep");

    if has_tool_phrase {
        "api_tool_lookup"
    } else if has_symbol_shape && token_count <= 3 {
        "symbol_lookup"
    } else if has_grep_shape {
        "content_grep"
    } else if token_count >= 5 {
        "conceptual_code"
    } else if token_count >= 2 {
        "code_investigation"
    } else {
        "unknown"
    }
}
