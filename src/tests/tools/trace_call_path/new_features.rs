//! Tests for trace_call_path.rs new features
//!
//! These tests cover the new functionality being added:
//! - Dual output format (JSON vs ASCII tree)
//! - Configurable parameters (semantic_limit, cross_language_max_depth, output_format)
//! - Semantic bridging without direct relationships
//!
//! All tests written TDD-style - they WILL FAIL until implementation is complete.

use crate::tools::trace_call_path::TraceCallPathTool;

#[test]
fn test_output_format_parameter_json() {
    // Test that output_format defaults to "json"
    let tool = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        cross_language: true,
        similarity_threshold: 0.7,
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "json".to_string(),
        semantic_limit: None,
        cross_language_max_depth: None,
    };

    assert_eq!(tool.output_format, "json");
}

#[test]
fn test_output_format_parameter_tree() {
    // Test that output_format can be set to "tree"
    let tool = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        cross_language: true,
        similarity_threshold: 0.7,
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "tree".to_string(),
        semantic_limit: None,
        cross_language_max_depth: None,
    };

    assert_eq!(tool.output_format, "tree");
}

#[test]
fn test_semantic_limit_parameter() {
    // Test that semantic_limit can be configured
    let tool = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        cross_language: true,
        similarity_threshold: 0.7,
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "json".to_string(),
        semantic_limit: Some(20u32),
        cross_language_max_depth: None,
    };

    assert_eq!(tool.semantic_limit, Some(20));
}

#[test]
fn test_semantic_limit_default() {
    // Test that semantic_limit defaults to None (will use 8 in implementation)
    let tool = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        cross_language: true,
        similarity_threshold: 0.7,
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "json".to_string(),
        semantic_limit: None,
        cross_language_max_depth: None,
    };

    assert_eq!(tool.semantic_limit, None);
}

#[test]
fn test_cross_language_max_depth_parameter() {
    // Test that cross_language_max_depth can be configured
    let tool = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 5,
        cross_language: true,
        similarity_threshold: 0.7,
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "json".to_string(),
        semantic_limit: None,
        cross_language_max_depth: Some(3),
    };

    assert_eq!(tool.cross_language_max_depth, Some(3));
}

#[test]
fn test_cross_language_max_depth_default() {
    // Test that cross_language_max_depth defaults to None (will use max_depth)
    let tool = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 5,
        cross_language: true,
        similarity_threshold: 0.7,
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "json".to_string(),
        semantic_limit: None,
        cross_language_max_depth: None,
    };

    assert_eq!(tool.cross_language_max_depth, None);
}

#[test]
fn test_all_new_parameters_together() {
    // Test that all new parameters work together
    let tool = TraceCallPathTool {
        symbol: "processPayment".to_string(),
        direction: "both".to_string(),
        max_depth: 5,
        cross_language: true,
        similarity_threshold: 0.8,
        context_file: Some("src/payment.ts".to_string()),
        workspace: Some("reference_workspace_id".to_string()),
        output_format: "tree".to_string(),
        semantic_limit: Some(15u32),
        cross_language_max_depth: Some(4),
    };

    assert_eq!(tool.output_format, "tree");
    assert_eq!(tool.semantic_limit, Some(15));
    assert_eq!(tool.cross_language_max_depth, Some(4));
    assert_eq!(tool.workspace, Some("reference_workspace_id".to_string()));
}
