use crate::extractors::{Symbol, SymbolKind};
use crate::handler::search_telemetry;
use crate::handler::tool_targets;
use crate::tools::search::FastSearchTool;
use crate::tools::search::trace::{SearchExecutionKind, SearchExecutionResult, SearchHit};
use crate::tools::{DeepDiveTool, GetSymbolsTool};

fn sample_symbol() -> Symbol {
    Symbol {
        id: "sym_1".to_string(),
        name: "search_handler".to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/dashboard/routes/search.rs".to_string(),
        start_line: 42,
        start_column: 0,
        end_line: 42,
        end_column: 24,
        start_byte: 0,
        end_byte: 24,
        signature: Some("fn search_handler()".to_string()),
        doc_comment: None,
        visibility: None,
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: Some(7.5),
        code_context: None,
        content_type: None,
    }
}

#[test]
fn test_fast_search_metadata_captures_trace_and_intent() {
    let params = FastSearchTool {
        query: "find references for search handler".to_string(),
        search_target: "definitions".to_string(),
        language: Some("rust".to_string()),
        file_pattern: Some("src/**/*.rs".to_string()),
        limit: 10,
        ..Default::default()
    };
    let hit = SearchHit::from_symbol(sample_symbol(), "workspace-a".to_string());
    let execution = SearchExecutionResult::new(
        vec![hit],
        true,
        4,
        "fast_search_definitions",
        SearchExecutionKind::Definitions,
    );

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["intent"], "api_tool_lookup");
    assert_eq!(
        metadata["normalized_query"],
        "find references for search handler"
    );
    assert_eq!(metadata["trace"]["strategy"], "fast_search_definitions");
    assert_eq!(metadata["trace"]["returned_hit_count"], 1);
    assert_eq!(metadata["trace"]["result_count"], 4);
    assert_eq!(metadata["trace"]["top_hits"][0]["name"], "search_handler");
}

#[test]
fn test_get_symbols_metadata_prefers_file_target_with_symbol_filter() {
    let params = GetSymbolsTool {
        file_path: "src/dashboard/routes/search.rs".to_string(),
        max_depth: 1,
        target: Some("run_search".to_string()),
        limit: Some(10),
        mode: Some("minimal".to_string()),
        workspace: Some("primary".to_string()),
    };

    let metadata = tool_targets::get_symbols_metadata(&params);

    assert_eq!(metadata["target"]["target_symbol_name"], "run_search");
    assert_eq!(
        metadata["target"]["target_file_path"],
        "src/dashboard/routes/search.rs"
    );
}

#[test]
fn test_deep_dive_metadata_carries_symbol_and_context_file_target() {
    let params = DeepDiveTool {
        symbol: "search_handler".to_string(),
        depth: "context".to_string(),
        context_file: Some("src/dashboard/routes/search.rs".to_string()),
        workspace: Some("primary".to_string()),
    };

    let metadata = tool_targets::deep_dive_metadata(&params);

    assert_eq!(metadata["target"]["target_symbol_name"], "search_handler");
    assert_eq!(
        metadata["target"]["target_file_path"],
        "src/dashboard/routes/search.rs"
    );
}
