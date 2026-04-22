use crate::extractors::{Symbol, SymbolKind};
use crate::handler::search_telemetry;
use crate::handler::tool_targets;
use crate::tools::navigation::CallPathTool;
use crate::tools::editing::rewrite_symbol::RewriteSymbolTool;
use crate::tools::search::FastSearchTool;
use crate::tools::search::trace::{
    FilePatternDiagnostic, HintKind, SearchExecutionKind, SearchExecutionResult, SearchHit,
    ZeroHitReason,
};
use crate::tools::spillover::SpilloverGetTool;
use crate::tools::{BlastRadiusTool, DeepDiveTool, GetSymbolsTool};

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
    assert!(
        metadata["trace"]["zero_hit_reason"].is_null(),
        "execution with hits should serialize zero_hit_reason as null"
    );
    assert!(
        metadata["trace"]["file_pattern_diagnostic"].is_null(),
        "execution with hits should serialize file_pattern_diagnostic as null"
    );
    assert!(
        metadata["trace"]["hint_kind"].is_null(),
        "execution without a hint should serialize hint_kind as null"
    );
}

#[test]
fn test_fast_search_metadata_serializes_zero_hit_reason() {
    let params = FastSearchTool {
        query: "nonexistent".to_string(),
        search_target: "content".to_string(),
        file_pattern: Some("src/ui/**".to_string()),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_content",
        SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        },
    );
    execution.trace.zero_hit_reason = Some(ZeroHitReason::FilePatternFiltered);

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(
        metadata["trace"]["zero_hit_reason"],
        "file_pattern_filtered"
    );
}

#[test]
fn test_fast_search_metadata_serializes_file_pattern_diagnostic() {
    let params = FastSearchTool {
        query: "calculate_total".to_string(),
        search_target: "content".to_string(),
        file_pattern: Some("src/** tests/**".to_string()),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_content",
        SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        },
    );
    execution.trace.file_pattern_diagnostic =
        Some(FilePatternDiagnostic::WhitespaceSeparatedMultiGlob);

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(
        metadata["trace"]["file_pattern_diagnostic"],
        "whitespace_separated_multi_glob"
    );
}

#[test]
fn test_fast_search_metadata_serializes_scoped_file_pattern_diagnostic() {
    let params = FastSearchTool {
        query: "calculate_total".to_string(),
        search_target: "content".to_string(),
        file_pattern: Some("src/ui/**".to_string()),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_content",
        SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        },
    );
    execution.trace.file_pattern_diagnostic = Some(FilePatternDiagnostic::NoInScopeCandidates);

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(
        metadata["trace"]["file_pattern_diagnostic"],
        "no_in_scope_candidates"
    );
}

#[test]
fn test_fast_search_metadata_serializes_hint_kind() {
    let params = FastSearchTool {
        query: "retry backoff jitter".to_string(),
        search_target: "content".to_string(),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_content",
        SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        },
    );
    execution.trace.hint_kind = Some(HintKind::FilePatternSyntaxHint);

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["trace"]["hint_kind"], "file_pattern_syntax_hint");
}

#[test]
fn test_fast_search_metadata_serializes_out_of_scope_hint_kind() {
    let params = FastSearchTool {
        query: "marker scope".to_string(),
        search_target: "content".to_string(),
        file_pattern: Some("src/ui/**".to_string()),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_content",
        SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        },
    );
    execution.trace.hint_kind = Some(HintKind::OutOfScopeContentHint);

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["trace"]["hint_kind"], "out_of_scope_content_hint");
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

#[test]
fn test_spillover_get_metadata_carries_handle_and_limit() {
    let params = SpilloverGetTool {
        spillover_handle: "br_123".to_string(),
        limit: Some(5),
        format: Some("compact".to_string()),
    };

    let metadata = tool_targets::spillover_get_metadata(&params);

    assert_eq!(metadata["spillover_handle"], "br_123");
    assert_eq!(metadata["limit"], 5);
    assert_eq!(metadata["format"], "compact");
}

#[test]
fn test_blast_radius_metadata_carries_seed_modes() {
    let params = BlastRadiusTool {
        symbol_ids: vec!["sym_a".to_string()],
        file_paths: vec!["src/api.rs".to_string()],
        from_revision: Some(10),
        to_revision: Some(12),
        max_depth: 2,
        limit: 5,
        include_tests: true,
        format: Some("readable".to_string()),
        workspace: Some("primary".to_string()),
    };

    let metadata = tool_targets::blast_radius_metadata(&params);

    assert_eq!(metadata["symbol_ids"][0], "sym_a");
    assert_eq!(metadata["file_paths"][0], "src/api.rs");
    assert_eq!(metadata["from_revision"], 10);
    assert_eq!(metadata["to_revision"], 12);
    assert_eq!(metadata["limit"], 5);
    assert_eq!(metadata["format"], "readable");
}

#[test]
fn test_call_path_metadata_carries_file_path_hints() {
    let params = CallPathTool {
        from: "entry".to_string(),
        to: "target".to_string(),
        max_hops: 4,
        workspace: Some("primary".to_string()),
        from_file_path: Some("src/a.rs".to_string()),
        to_file_path: Some("src/b.rs".to_string()),
    };

    let metadata = tool_targets::call_path_metadata(&params);

    assert_eq!(metadata["from"], "entry");
    assert_eq!(metadata["to"], "target");
    assert_eq!(metadata["from_file_path"], "src/a.rs");
    assert_eq!(metadata["to_file_path"], "src/b.rs");
    assert_eq!(metadata["target"]["target_file_path"], "src/b.rs");
}

#[test]
fn test_rewrite_symbol_metadata_carries_disambiguation_and_dry_run() {
    let params = RewriteSymbolTool {
        symbol: "AuthService::validate".to_string(),
        operation: "replace_body".to_string(),
        content: "{ validate_claims(token)?; }".to_string(),
        file_path: Some("src/auth.rs".to_string()),
        workspace: Some("primary".to_string()),
        dry_run: true,
    };

    let metadata = tool_targets::rewrite_symbol_metadata(&params);

    assert_eq!(metadata["symbol"], "AuthService::validate");
    assert_eq!(metadata["operation"], "replace_body");
    assert_eq!(metadata["dry_run"], true);
    assert_eq!(metadata["file_path"], "src/auth.rs");
    assert_eq!(metadata["target"]["target_file_path"], "src/auth.rs");
}
