// Tests extracted from src/tools/search.rs
// These were previously inline tests that have been moved to follow project standards

mod auto_detection_tests;
mod hybrid_search_tests;
mod lean_format_tests;
mod line_mode;
mod primary_workspace_bug;
mod quality;
mod race_condition;
mod semantic_error_handling_tests;
mod semantic_scoring_tests;
mod tantivy_language_config_tests;

use crate::tools::search::*;

#[test]
fn test_preprocess_fallback_query_multi_word() {
    let tool = FastSearchTool {
        query: "user authentication".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: None,
        search_target: "content".to_string(),
        output: None,
        context_lines: None,
        output_format: None,
    };

    assert_eq!(
        tool.preprocess_fallback_query("user authentication"),
        "user authentication",
        "Multi-word queries should use implicit AND (space-separated)"
    );
}

#[test]
fn test_preprocess_fallback_query_single_word() {
    let tool = FastSearchTool {
        query: "getUserData".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: None,
        search_target: "content".to_string(),
        output: None,
        context_lines: None,
        output_format: None,
    };

    assert_eq!(
        tool.preprocess_fallback_query("getUserData"),
        "getUserData",
        "Single words should remain unchanged"
    );
}

#[test]
fn test_preprocess_fallback_query_quoted() {
    let tool = FastSearchTool {
        query: "\"exact match\"".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: None,
        search_target: "content".to_string(),
        output: None,
        context_lines: None,
        output_format: None,
    };

    assert_eq!(
        tool.preprocess_fallback_query("\"exact match\""),
        "\"exact match\"",
        "Quoted queries should remain unchanged"
    );
}

#[test]
fn test_preprocess_fallback_query_exclusion() {
    let tool = FastSearchTool {
        query: "user -password".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: None,
        search_target: "content".to_string(),
        output: None,
        context_lines: None,
        output_format: None,
    };

    assert_eq!(
        tool.preprocess_fallback_query("user -password"),
        "user",
        "Extract only positive terms for FTS5 (file-level search), exclusions handled by line_match_strategy (line-level filtering)"
    );
}

#[test]
fn test_preprocess_fallback_query_wildcard() {
    let tool = FastSearchTool {
        query: "getUser*".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: None,
        search_target: "content".to_string(),
        output: None,
        context_lines: None,
        output_format: None,
    };

    assert_eq!(
        tool.preprocess_fallback_query("getUser*"),
        "getUser*",
        "Wildcard queries should remain unchanged"
    );
}

#[test]
fn test_preprocess_fallback_query_explicit_or() {
    let tool = FastSearchTool {
        query: "getUserData OR fetchUserData".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: None,
        search_target: "content".to_string(),
        output: None,
        context_lines: None,
        output_format: None,
    };

    assert_eq!(
        tool.preprocess_fallback_query("getUserData OR fetchUserData"),
        "getUserData OR fetchUserData",
        "Explicit OR operators should pass through"
    );
}

#[test]
fn test_preprocess_fallback_query_explicit_and() {
    let tool = FastSearchTool {
        query: "user AND authentication".to_string(),
        search_method: "text".to_string(),
        language: None,
        file_pattern: None,
        limit: 15,
        workspace: None,
        search_target: "content".to_string(),
        output: None,
        context_lines: None,
        output_format: None,
    };

    assert_eq!(
        tool.preprocess_fallback_query("user AND authentication"),
        "user AND authentication",
        "Explicit AND operators should pass through"
    );
}
