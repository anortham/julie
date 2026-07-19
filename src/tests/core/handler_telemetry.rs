use std::fs;
use std::time::Duration;

use crate::extractors::{Symbol, SymbolKind};
use crate::handler::JulieServerHandler;
use crate::handler::search_telemetry;
use crate::handler::tool_targets;
use crate::search::index::{FileMatchKind, FileSearchResult};
use crate::tools::ManageWorkspaceTool;
use crate::tools::editing::rewrite_symbol::RewriteSymbolTool;
use crate::tools::navigation::CallPathTool;
use crate::tools::navigation::FastRefsTool;
use crate::tools::patterns::{PatternsFormat, PatternsGroupBy, PatternsOperation, PatternsTool};
use crate::tools::search::FastSearchTool;
use crate::tools::search::trace::{
    FilePatternDiagnostic, HintKind, LineEnrichmentStatus, SearchExecutionKind,
    SearchExecutionResult, SearchHit, ZeroHitReason,
};
use crate::tools::spillover::SpilloverGetTool;
use crate::tools::{BlastRadiusTool, DeepDiveDepth, DeepDiveTool, GetContextTool, GetSymbolsTool};
use tempfile::TempDir;

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
        body_span: None,
        body_hash: None,
        annotations: Vec::new(),
    }
}

fn sample_file_hit() -> SearchHit {
    SearchHit::from_file_result(
        FileSearchResult {
            file_path: "src/tools/search/mod.rs".to_string(),
            language: "rust".to_string(),
            score: 12.0,
            match_kind: FileMatchKind::ExactPath,
        },
        "workspace-a".to_string(),
    )
}

async fn seed_workspace(files: &[(&str, &str)]) -> (TempDir, JulieServerHandler) {
    unsafe {
        std::env::set_var("JULIE_SKIP_SEARCH_INDEX", "0");
    }

    let temp_dir = TempDir::new().expect("tempdir");
    let workspace_path = temp_dir.path().to_path_buf();

    for (rel_path, content) in files {
        let full = workspace_path.join(rel_path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(full, content).expect("write file");
    }

    let handler = JulieServerHandler::new_for_test()
        .await
        .expect("handler for test");
    handler
        .initialize_workspace_with_force(Some(workspace_path.to_string_lossy().to_string()), true)
        .await
        .expect("initialize workspace");

    let index_tool = ManageWorkspaceTool {
        operation: "index".to_string(),
        path: Some(workspace_path.to_string_lossy().to_string()),
        force: Some(false),
        name: None,
        workspace_id: None,
        detailed: None,
    };
    index_tool
        .call_tool(&handler)
        .await
        .expect("index workspace");
    tokio::time::sleep(Duration::from_millis(500)).await;

    (temp_dir, handler)
}

fn search_tool(query: &str, _search_target: &str) -> FastSearchTool {
    FastSearchTool {
        query: query.to_string(),
        language: None,
        file_pattern: None,
        limit: 10,
        context_lines: Some(0),
        exclude_tests: None,
        backend: None,
        workspace: Some("primary".to_string()),
        return_format: "full".to_string(),
    }
}

#[test]
fn test_fast_search_metadata_captures_trace_and_intent() {
    let params = FastSearchTool {
        query: "find references for search handler".to_string(),
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
    assert_eq!(metadata["trace"]["scope_relaxed"], false);
    assert!(metadata["trace"]["original_file_pattern"].is_null());
    assert!(metadata["trace"]["original_zero_hit_reason"].is_null());
    assert_eq!(metadata["trace"]["scope_rescue_count"], 0);
    assert_eq!(metadata["trace"]["or_disjunction_detected"], false);
}

#[tokio::test]
async fn test_fast_search_metadata_serializes_unified_trace_fields() {
    // After T8, all traffic routes through execute_search_unified.
    // This verifies unified execution metadata keeps the stable trace fields
    // callers rely on, independent of content-specific enrichment status.
    let (_temp_dir, handler) = seed_workspace(&[(
        "src/lib.rs",
        "pub fn search_handler() { let marker_token = 1; }\n",
    )])
    .await;

    let tool = search_tool("marker_token", "content");
    let execution = tool
        .execute_with_trace(&handler)
        .await
        .expect("unified search should not error")
        .execution
        .expect("execute_with_trace should populate execution trace");
    let metadata = search_telemetry::fast_search_metadata(&tool, Some(&execution));
    let trace = metadata["trace"]
        .as_object()
        .expect("trace metadata should be an object");

    // Core trace fields that the unified path populates.
    assert!(
        trace.contains_key("strategy"),
        "strategy should be serialized"
    );
    assert!(
        trace.contains_key("returned_hit_count"),
        "returned_hit_count should be serialized"
    );
    assert!(
        trace.contains_key("result_count"),
        "result_count should be serialized"
    );
    assert!(
        trace.contains_key("kind_distribution"),
        "kind_distribution should be serialized after T8"
    );
    // marker_token is a variable identifier; unified search should find it.
    assert!(
        metadata["trace"]["returned_hit_count"]
            .as_u64()
            .unwrap_or(0)
            >= 1,
        "unified search for 'marker_token' should return at least one hit"
    );
}

#[tokio::test]
async fn test_fast_search_metadata_serializes_definition_exact_match_field() {
    let (_temp_dir, handler) = seed_workspace(&[(
        "src/lib.rs",
        "pub fn search_handler() {}\npub fn search_index() {}\n",
    )])
    .await;

    let tool = search_tool("search_handler", "definitions");
    let execution = tool
        .execute_with_trace(&handler)
        .await
        .expect("definition search should not error")
        .execution
        .expect("execute_with_trace should populate execution for definition search");
    assert!(
        !execution.hits.is_empty(),
        "expected exact-match definition search to return at least one hit"
    );

    let metadata = search_telemetry::fast_search_metadata(&tool, Some(&execution));
    let trace = metadata["trace"]
        .as_object()
        .expect("trace metadata should be an object");

    assert!(
        trace.contains_key("definition_exact_match"),
        "definition_exact_match should be serialized for definitions searches"
    );
    assert_eq!(metadata["trace"]["definition_exact_match"], true);
}

#[test]
fn test_fast_search_metadata_captures_workspace_param() {
    let params = FastSearchTool {
        query: "search_handler".to_string(),
        workspace: Some("target-workspace".to_string()),
        limit: 0,
        ..Default::default()
    };

    let metadata = search_telemetry::fast_search_metadata(&params, None);

    assert_eq!(metadata["workspace"], "target-workspace");
    assert_eq!(metadata["limit"], 1);
}

#[test]
fn test_fast_refs_metadata_captures_result_shaping_fields() {
    let params = FastRefsTool {
        symbol: "Command".to_string(),
        include_definition: false,
        limit: 25,
        workspace: Some("target-workspace".to_string()),
        reference_kind: Some("call".to_string()),
    };

    let metadata = tool_targets::fast_refs_metadata(&params);

    assert_eq!(metadata["symbol"], "Command");
    assert_eq!(metadata["include_definition"], false);
    assert_eq!(metadata["limit"], 25);
    assert_eq!(metadata["workspace"], "target-workspace");
    assert_eq!(metadata["reference_kind"], "call");
    assert_eq!(metadata["target"]["target_symbol_name"], "Command");
}

#[test]
fn test_fast_search_metadata_serializes_zero_hit_reason() {
    let params = FastSearchTool {
        query: "nonexistent".to_string(),
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
fn test_fast_search_metadata_serializes_request_level_file_pattern_diagnostic() {
    let params = FastSearchTool {
        query: "calculate_total".to_string(),
        file_pattern: Some("src/** docs/**".to_string()),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_input_diagnostic",
        SearchExecutionKind::Definitions,
    );
    execution.trace.file_pattern_diagnostic =
        Some(FilePatternDiagnostic::WhitespaceSeparatedMultiGlob);
    execution.trace.hint_kind = Some(HintKind::FilePatternSyntaxHint);

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(
        metadata["trace"]["strategy"],
        "fast_search_input_diagnostic"
    );
    assert_eq!(metadata["trace"]["returned_hit_count"], 0);
    assert_eq!(
        metadata["trace"]["file_pattern_diagnostic"],
        "whitespace_separated_multi_glob"
    );
    assert_eq!(metadata["trace"]["hint_kind"], "file_pattern_syntax_hint");
    assert!(metadata["trace"]["zero_hit_reason"].is_null());
}

#[test]
fn test_fast_search_metadata_serializes_hint_kind() {
    let params = FastSearchTool {
        query: "retry backoff jitter".to_string(),
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
fn task2_target_hints_serializes_trace_metadata() {
    let cases = [
        (
            "fixtures/real-world/php/index.php",
            HintKind::FileTargetHint,
            "file_target_hint",
        ),
        (
            "src/tools/search/mod.rs",
            HintKind::FileTargetHint,
            "file_target_hint",
        ),
        (
            "ArgAction::SetTrue",
            HintKind::DefinitionsTargetHint,
            "definitions_target_hint",
        ),
        (
            "OS.has_feature",
            HintKind::DefinitionsTargetHint,
            "definitions_target_hint",
        ),
        (
            "format_line_mode_output",
            HintKind::DefinitionsTargetHint,
            "definitions_target_hint",
        ),
        (
            "fast_refs_metadata(",
            HintKind::DefinitionsTargetHint,
            "definitions_target_hint",
        ),
    ];

    for (query, hint_kind, expected) in cases {
        let params = FastSearchTool {
            query: query.to_string(),
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
        execution.trace.hint_kind = Some(hint_kind);

        let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

        assert_eq!(metadata["trace"]["hint_kind"], expected);
    }
}

#[test]
fn test_fast_search_metadata_serializes_out_of_scope_hint_kind() {
    let params = FastSearchTool {
        query: "marker scope".to_string(),
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
fn test_fast_search_metadata_serializes_scope_rescue_fields() {
    let params = FastSearchTool {
        query: "marker_scope".to_string(),
        file_pattern: Some("src/ui/**".to_string()),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        2,
        "fast_search_content",
        SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        },
    );
    execution.trace.scope_relaxed = true;
    execution.trace.original_file_pattern = Some("src/ui/**".to_string());
    execution.trace.original_zero_hit_reason = Some(ZeroHitReason::FilePatternFiltered);
    execution.trace.scope_rescue_count = 1;

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["trace"]["scope_relaxed"], true);
    assert_eq!(metadata["trace"]["original_file_pattern"], "src/ui/**");
    assert_eq!(
        metadata["trace"]["original_zero_hit_reason"],
        "file_pattern_filtered"
    );
    assert_eq!(metadata["trace"]["scope_rescue_count"], 1);
}

#[test]
fn test_fast_search_metadata_serializes_or_disjunction_detection() {
    let params = FastSearchTool {
        query: "logging.basicConfig OR datefmt".to_string(),
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
            file_level: true,
        },
    );
    execution.trace.or_disjunction_detected = true;

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["trace"]["or_disjunction_detected"], true);
}

#[test]
fn test_fast_search_metadata_serializes_backend_fallback() {
    let params = FastSearchTool {
        query: "lexical_backend_marker".to_string(),
        limit: 10,
        backend: Some(crate::tools::search::SearchBackend::Semantic),
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_unified",
        SearchExecutionKind::Definitions,
    );
    execution.trace.backend_fallback = true;

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["trace"]["backend_fallback"], true);
}

#[test]
fn test_fast_search_metadata_serializes_line_enrichment_fields() {
    let params = FastSearchTool {
        query: "line enrichment marker".to_string(),
        limit: 10,
        ..Default::default()
    };
    let mut execution = SearchExecutionResult::new(
        Vec::new(),
        false,
        0,
        "fast_search_unified",
        SearchExecutionKind::Content {
            workspace_label: Some("primary".to_string()),
            file_level: false,
        },
    );
    execution.trace.line_match_strategy = Some("tokens".to_string());
    execution.trace.line_enrichment_status = Some(LineEnrichmentStatus::Failed);
    execution.trace.line_enrichment_match_count = Some(0);
    execution.trace.line_enrichment_zero_hit_reason = Some(ZeroHitReason::LineMatchMiss);
    execution.trace.line_enrichment_file_pattern_diagnostic =
        Some(FilePatternDiagnostic::NoInScopeCandidates);
    execution.trace.line_enrichment_error = Some("line index unavailable".to_string());

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["trace"]["line_match_strategy"], "tokens");
    assert_eq!(metadata["trace"]["line_enrichment_status"], "failed");
    assert_eq!(metadata["trace"]["line_enrichment_match_count"], 0);
    assert_eq!(
        metadata["trace"]["line_enrichment_zero_hit_reason"],
        "line_match_miss"
    );
    assert_eq!(
        metadata["trace"]["line_enrichment_file_pattern_diagnostic"],
        "no_in_scope_candidates"
    );
    assert_eq!(
        metadata["trace"]["line_enrichment_error"],
        "line index unavailable"
    );
}

#[tokio::test]
async fn test_fast_search_metadata_serializes_or_fallback_relaxed_from_execution() {
    let (_temp_dir, handler) = seed_workspace(&[
        (
            "src/alpha.rs",
            "pub fn alpha_only_marker() { let alpha_relaxed_marker = 1; }\n",
        ),
        (
            "src/beta.rs",
            "pub fn beta_only_marker() { let beta_relaxed_marker = 1; }\n",
        ),
    ])
    .await;

    let tool = search_tool("alpha_relaxed_marker beta_relaxed_marker", "content");
    let execution = tool
        .execute_with_trace(&handler)
        .await
        .expect("OR fallback search should not error")
        .execution
        .expect("execute_with_trace should populate execution");

    assert!(
        !execution.hits.is_empty(),
        "OR fallback should supply partial-match hits",
    );
    assert!(
        execution.relaxed,
        "SearchIndex OR fallback relaxed flag must reach FastSearchTool execution",
    );

    let metadata = search_telemetry::fast_search_metadata(&tool, Some(&execution));
    assert_eq!(
        metadata["trace"]["relaxed"], true,
        "telemetry must serialize the propagated relaxed flag",
    );
}

#[test]
fn test_fast_search_metadata_uses_file_lookup_intent_for_files_query() {
    // After T8, search_target is gone. Intent is inferred from query shape.
    let params = FastSearchTool {
        query: "line_mode.rs".to_string(),
        ..Default::default()
    };

    let metadata = search_telemetry::fast_search_metadata(&params, None);

    // A filename query gets "unknown" intent (no symbol shape, no tool phrase)
    assert!(
        metadata["intent"].is_string(),
        "intent should be serialized"
    );
}

#[test]
fn test_fast_search_metadata_serializes_file_hit_trace() {
    let params = FastSearchTool {
        query: "src/tools/search/mod.rs".to_string(),
        ..Default::default()
    };
    let execution = SearchExecutionResult::new(
        vec![sample_file_hit()],
        false,
        1,
        "fast_search_files",
        SearchExecutionKind::Files,
    );

    let metadata = search_telemetry::fast_search_metadata(&params, Some(&execution));

    assert_eq!(metadata["trace"]["strategy"], "fast_search_files");
    assert_eq!(metadata["trace"]["top_hits"][0]["kind"], "file");
    assert_eq!(
        metadata["trace"]["top_hits"][0]["file"],
        "src/tools/search/mod.rs"
    );
    assert!(metadata["trace"]["top_hits"][0]["line"].is_null());
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
        depth: DeepDiveDepth::Context,
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
fn test_get_context_metadata_carries_format_and_task_inputs() {
    let params = GetContextTool {
        query: "workspace routing".to_string(),
        max_tokens: Some(2400),
        workspace: Some("target-workspace".to_string()),
        language: Some("rust".to_string()),
        file_pattern: Some("src/**/*.rs".to_string()),
        format: Some("readable".to_string()),
        edited_files: Some(vec!["src/handler.rs".to_string()]),
        entry_symbols: Some(vec!["JulieServerHandler::get_context".to_string()]),
        stack_trace: Some("src/handler.rs:2656".to_string()),
        failing_test: Some("get_context_metadata".to_string()),
        max_hops: Some(2),
        prefer_tests: Some(true),
    };

    let metadata = tool_targets::get_context_metadata(&params);

    assert_eq!(metadata["query"], "workspace routing");
    assert_eq!(metadata["format"], "readable");
    assert_eq!(metadata["workspace"], "target-workspace");
    assert_eq!(metadata["edited_files"][0], "src/handler.rs");
    assert_eq!(
        metadata["entry_symbols"][0],
        "JulieServerHandler::get_context"
    );
    assert_eq!(metadata["max_hops"], 2);
    assert_eq!(metadata["prefer_tests"], true);
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

#[test]
fn test_patterns_metadata_carries_query_filters_and_workspace() {
    let params = PatternsTool {
        operation: PatternsOperation::Search,
        pattern_id: Some("http.client_request.v1".to_string()),
        query: Some("client".to_string()),
        path: Some("src/**".to_string()),
        language: Some("rust".to_string()),
        where_filter: Some("client=reqwest;method=GET".to_string()),
        facet: None,
        group_by: PatternsGroupBy::Directory,
        limit: 25,
        workspace: Some("target-workspace".to_string()),
        format: PatternsFormat::Json,
    };

    let metadata = tool_targets::patterns_metadata(&params);

    assert_eq!(metadata["operation"], "search");
    assert_eq!(metadata["pattern_id"], "http.client_request.v1");
    assert_eq!(metadata["query"], "client");
    assert_eq!(metadata["path"], "src/**");
    assert_eq!(metadata["language"], "rust");
    assert_eq!(metadata["where"], "client=reqwest;method=GET");
    assert_eq!(metadata["group_by"], "directory");
    assert_eq!(metadata["limit"], 25);
    assert_eq!(metadata["workspace"], "target-workspace");
    assert_eq!(metadata["format"], "json");
    assert_eq!(metadata["target"]["target_file_path"], "src/**");
}
