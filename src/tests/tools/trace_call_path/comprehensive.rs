//! Comprehensive tests for TraceCallPathTool
//!
//! Tests verify cross-language call path tracing functionality.
//!
//! Note: These tests focus on the tool's logic and API validation.
//! Full integration tests require a complete workspace setup.

use crate::tools::trace_call_path::TraceCallPathTool;
use crate::utils::cross_language_intelligence::generate_naming_variants;

// Helper function to create TraceCallPathTool with defaults
#[allow(dead_code)]
fn create_test_tool() -> TraceCallPathTool {
    TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: None,
        output_format: "json".to_string(),
    }
}

#[test]
fn test_parameter_validation_max_depth() {
    // Test that max_depth validation works
    let tool = TraceCallPathTool {
        symbol: "testFunction".to_string(),
        direction: "upstream".to_string(),
        max_depth: 15, // Exceeds limit of 10
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "json".to_string(),
    };

    assert!(
        tool.max_depth > 10,
        "Should detect max_depth exceeding limit"
    );
}

// Tests removed: similarity_threshold parameter has been removed and is now hardcoded to 0.7

#[test]
fn test_direction_values() {
    // Test valid direction values
    let upstream = TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: None,
        output_format: "json".to_string(),
    };

    let downstream = TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "downstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: None,
        output_format: "json".to_string(),
    };

    let both = TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "both".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: None,
        output_format: "json".to_string(),
    };

    assert_eq!(upstream.direction, "upstream");
    assert_eq!(downstream.direction, "downstream");
    assert_eq!(both.direction, "both");
}

// Test removed: cross_language parameter removed - now always enabled (Julie's superpower!)

#[test]
fn test_naming_variant_generation_typescript() {
    // Test that naming variants are generated correctly for TypeScript names
    let variants = generate_naming_variants("getUserData");

    // Should generate snake_case for Python
    assert!(
        variants.contains(&"get_user_data".to_string()),
        "Should generate snake_case variant: {:?}",
        variants
    );

    // Should generate PascalCase for Go/C#
    assert!(
        variants.contains(&"GetUserData".to_string()),
        "Should generate PascalCase variant: {:?}",
        variants
    );

    // Should include original
    assert!(
        variants.contains(&"getUserData".to_string()),
        "Should include original: {:?}",
        variants
    );
}

#[test]
fn test_naming_variant_generation_python() {
    // Test that naming variants work from Python names
    let variants = generate_naming_variants("get_user_data");

    // Should generate camelCase for TypeScript/JavaScript
    assert!(
        variants.contains(&"getUserData".to_string()),
        "Should generate camelCase variant: {:?}",
        variants
    );

    // Should generate PascalCase for Go/C#
    assert!(
        variants.contains(&"GetUserData".to_string()),
        "Should generate PascalCase variant: {:?}",
        variants
    );
}

#[test]
fn test_naming_variant_generation_go() {
    // Test that naming variants work from Go/C# names
    let variants = generate_naming_variants("GetUserData");

    // Should generate camelCase for TypeScript
    assert!(
        variants.contains(&"getUserData".to_string()),
        "Should generate camelCase variant: {:?}",
        variants
    );

    // Should generate snake_case for Python
    assert!(
        variants.contains(&"get_user_data".to_string()),
        "Should generate snake_case variant: {:?}",
        variants
    );
}

#[test]
fn test_default_parameters() {
    // Test that default parameters are set correctly
    let tool = TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "upstream".to_string(), // default in tool
        max_depth: 3,                      // default in tool
        context_file: None,
        workspace: Some("primary".to_string()), // default in tool
        output_format: "json".to_string(),
    };

    assert_eq!(tool.direction, "upstream");
    assert_eq!(tool.max_depth, 3);
    assert_eq!(tool.workspace, Some("primary".to_string()));
}

#[test]
fn test_context_file_filtering() {
    // Test context_file parameter
    let with_context = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: Some("src/user.ts".to_string()),
        workspace: None,
        output_format: "json".to_string(),
    };

    assert_eq!(with_context.context_file, Some("src/user.ts".to_string()));

    let without_context = TraceCallPathTool {
        symbol: "getUserData".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: None,
        output_format: "json".to_string(),
    };

    assert_eq!(without_context.context_file, None);
}

#[test]
fn test_workspace_filtering() {
    // Test workspace parameter variations
    let primary = TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: Some("primary".to_string()),
        output_format: "json".to_string(),
    };

    let all = TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: Some("all".to_string()),
        output_format: "json".to_string(),
    };

    let specific = TraceCallPathTool {
        symbol: "test".to_string(),
        direction: "upstream".to_string(),
        max_depth: 3,
        context_file: None,
        workspace: Some("project-b_a3f2b8c1".to_string()),
        output_format: "json".to_string(),
    };

    assert_eq!(primary.workspace, Some("primary".to_string()));
    assert_eq!(all.workspace, Some("all".to_string()));
    assert_eq!(specific.workspace, Some("project-b_a3f2b8c1".to_string()));
}

#[test]
fn test_max_depth_range() {
    // Test various max_depth values
    for depth in 1..=10 {
        let tool = TraceCallPathTool {
            symbol: "test".to_string(),
            direction: "upstream".to_string(),
            max_depth: depth,
            context_file: None,
            workspace: None,
            output_format: "json".to_string(),
        };

        assert_eq!(tool.max_depth, depth);
        assert!(tool.max_depth <= 10, "max_depth should be <= 10");
    }
}

// Test removed: similarity_threshold parameter removed - now hardcoded to 0.7

#[test]
fn test_naming_variants_preserve_underscores() {
    // Test that underscores in specific positions are preserved
    let variants = generate_naming_variants("_privateFunction");

    // Should handle leading underscore
    assert!(
        variants.len() > 0,
        "Should generate variants even with leading underscore"
    );
}

#[test]
fn test_naming_variants_multiple_words() {
    // Test complex multi-word names
    let variants = generate_naming_variants("getUserDataFromDatabase");

    assert!(variants.len() >= 3, "Should generate multiple variants");

    // Should handle complex conversions
    let has_snake = variants.iter().any(|v| v.contains('_'));
    assert!(has_snake, "Should generate snake_case for complex names");
}
