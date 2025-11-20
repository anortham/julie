/// Tests for shared tool infrastructure (src/tools/shared.rs)
/// Following TDD: Write tests first, then implement functionality

use crate::tools::shared::create_toonable_result;
use rust_mcp_sdk::schema::CallToolResult;
use serde::{Deserialize, Serialize};

/// Test data structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct MockResult {
    tool: String,
    items: Vec<String>,
    count: usize,
}

impl MockResult {
    fn new(tool: &str, items: Vec<String>) -> Self {
        let count = items.len();
        Self {
            tool: tool.to_string(),
            items,
            count,
        }
    }
}

#[test]
fn test_create_toonable_result_toon_mode_success() {
    // Arrange
    let result = MockResult::new("test_tool", vec!["item1".into(), "item2".into()]);

    // Act
    let call_result = create_toonable_result(
        &result,
        Some("toon"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");

    // Assert
    // Should have content with TOON format
    assert!(call_result.content.len() > 0, "Should have text content");
    // Should NOT have structured_content (TOON mode is text-only)
    assert!(call_result.structured_content.is_none(), "Should not have structured content in TOON mode");
}

#[test]
fn test_create_toonable_result_auto_mode_above_threshold() {
    // Arrange: 10 items, threshold 5 → should use TOON
    let items: Vec<String> = (0..10).map(|i| format!("item{}", i)).collect();
    let result = MockResult::new("test_tool", items);

    // Act
    let call_result = create_toonable_result(
        &result,
        Some("auto"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");

    // Assert
    // Should have text_content (TOON format)
    assert!(call_result.content.len() > 0, "Should have text content in auto mode above threshold");
    // Should NOT have structured_content
    assert!(call_result.structured_content.is_none(), "Should not have structured content when using TOON");
}

#[test]
fn test_create_toonable_result_auto_mode_below_threshold() {
    // Arrange: 3 items, threshold 5 → should use JSON
    let items: Vec<String> = (0..3).map(|i| format!("item{}", i)).collect();
    let result = MockResult::new("test_tool", items);

    // Act
    let call_result = create_toonable_result(
        &result,
        Some("auto"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");

    // Assert
    // Should have structured_content (JSON format)
    assert!(call_result.structured_content.is_some(), "Should have structured content in auto mode below threshold");
    // Should have empty text_content
    assert_eq!(call_result.content.len(), 0, "Should have empty text content when using JSON");
}

#[test]
fn test_create_toonable_result_auto_mode_at_threshold() {
    // Arrange: Exactly 5 items, threshold 5 → should use TOON
    let items: Vec<String> = (0..5).map(|i| format!("item{}", i)).collect();
    let result = MockResult::new("test_tool", items);

    // Act
    let call_result = create_toonable_result(
        &result,
        Some("auto"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");

    // Assert
    // Should have text_content (TOON format) - at threshold should trigger TOON
    assert!(call_result.content.len() > 0, "Should have text content at threshold");
    assert!(call_result.structured_content.is_none(), "Should not have structured content at threshold");
}

#[test]
fn test_create_toonable_result_json_mode() {
    // Arrange
    let result = MockResult::new("test_tool", vec!["item1".into(), "item2".into()]);

    // Act
    let call_result = create_toonable_result(
        &result,
        Some("json"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");

    // Assert
    // Should have structured_content (JSON format)
    assert!(call_result.structured_content.is_some(), "Should have structured content in JSON mode");
    // Should have empty text_content
    assert_eq!(call_result.content.len(), 0, "Should have empty text content in JSON mode");
}

#[test]
fn test_create_toonable_result_none_mode_defaults_to_json() {
    // Arrange
    let result = MockResult::new("test_tool", vec!["item1".into(), "item2".into()]);

    // Act
    let call_result = create_toonable_result(
        &result,
        None,
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");

    // Assert
    // Should have structured_content (JSON format is default)
    assert!(call_result.structured_content.is_some(), "Should have structured content when None");
    // Should have empty text_content
    assert_eq!(call_result.content.len(), 0, "Should have empty text content when None");
}

#[test]
fn test_create_toonable_result_fallback_to_json_on_toon_failure() {
    // This test will validate that when TOON encoding fails, we fall back to JSON
    // We'll use a data structure that toon_format might struggle with

    // For now, we'll test the happy path - once we implement the function,
    // we can add a test with intentionally malformed data that fails TOON encoding

    // Arrange
    let result = MockResult::new("test_tool", vec!["item1".into()]);

    // Act
    let call_result = create_toonable_result(
        &result,
        Some("toon"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed even if TOON fails");

    // Assert
    // Should always return a valid result (either TOON or JSON fallback)
    assert!(
        call_result.content.len() > 0 || call_result.structured_content.is_some(),
        "Should have either text or structured content"
    );
}

#[test]
fn test_create_toonable_result_validates_json_structure() {
    // Arrange
    let result = MockResult::new("test_tool", vec!["item1".into()]);

    // Act
    let call_result = create_toonable_result(
        &result,
        Some("json"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");

    // Assert
    let structured = call_result.structured_content.expect("Should have structured content");

    // Verify it's a valid JSON object with expected fields
    assert!(structured.contains_key("tool"), "Should have 'tool' field");
    assert!(structured.contains_key("items"), "Should have 'items' field");
    assert!(structured.contains_key("count"), "Should have 'count' field");
}

#[test]
fn test_create_toonable_result_different_thresholds() {
    // Test that auto mode respects different threshold values

    // Arrange: 8 items
    let items: Vec<String> = (0..8).map(|i| format!("item{}", i)).collect();
    let result = MockResult::new("test_tool", items);

    // Act & Assert with threshold 5 → should use TOON
    let result_threshold_5 = create_toonable_result(
        &result,
        Some("auto"),
        5,
        result.count,
        "test_tool"
    ).expect("Should succeed");
    assert!(result_threshold_5.content.len() > 0, "Should use TOON with threshold 5");

    // Act & Assert with threshold 10 → should use JSON
    let result_threshold_10 = create_toonable_result(
        &result,
        Some("auto"),
        10,
        result.count,
        "test_tool"
    ).expect("Should succeed");
    assert!(result_threshold_10.structured_content.is_some(), "Should use JSON with threshold 10");
}
