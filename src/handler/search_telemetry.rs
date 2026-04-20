use serde_json::{Value, json};

use crate::tools::search::FastSearchTool;
use crate::tools::search::trace::SearchExecutionResult;

const TRACE_VERSION: &str = "fast_search_trace_v1";

pub(crate) fn fast_search_metadata(
    params: &FastSearchTool,
    execution: Option<&SearchExecutionResult>,
) -> Value {
    let intent = infer_intent(&params.query, &params.search_target);
    let trace = execution.map(|result| {
        json!({
            "strategy": result.trace.strategy_id,
            "returned_hit_count": result.hits.len(),
            "result_count": result.total_results,
            "relaxed": result.relaxed,
            "top_hits": result.trace.top_hits,
        })
    });

    json!({
        "query": params.query,
        "normalized_query": normalize_query(&params.query),
        "search_target": params.search_target,
        "language": params.language,
        "file_pattern": params.file_pattern,
        "limit": params.limit,
        "exclude_tests": params.exclude_tests,
        "intent": intent,
        "trace_version": TRACE_VERSION,
        "trace": trace,
    })
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

fn infer_intent(query: &str, search_target: &str) -> &'static str {
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
    let has_grep_shape = search_target == "content"
        || normalized.contains("todo")
        || normalized.contains("fixme")
        || normalized.contains("grep");

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
