//! Tests for TOON format encoding in search results
//!
//! Validates TOON encoding works with Julie's actual data structures

use crate::extractors::base::{Symbol, SymbolKind, Visibility};
use crate::tools::search::formatting::encode_to_toon_with_fallback;
use crate::tools::shared::OptimizedResponse;

/// Helper to create a test symbol
fn create_test_symbol(name: &str, line: u32) -> Symbol {
    Symbol {
        id: format!("test_{}", name),
        name: name.to_string(),
        kind: SymbolKind::Function,
        language: "rust".to_string(),
        file_path: "src/test.rs".to_string(),
        start_line: line,
        end_line: line + 5,
        start_column: 0,
        end_column: 1,
        start_byte: (line * 100) as u32,
        end_byte: ((line + 5) * 100) as u32,
        signature: Some(format!("fn {}()", name)),
        doc_comment: Some(format!("Test function {}", name)),
        visibility: Some(Visibility::Public),
        parent_id: None,
        metadata: None,
        semantic_group: None,
        confidence: Some(0.9),
        code_context: Some(format!("fn {}() {{ }}", name)),
        content_type: None,
    }
}

#[test]
fn test_toon_encoding_basic() {
    let symbols = vec![create_test_symbol("getUserData", 10)];
    let response = OptimizedResponse::new("fast_search", symbols, 0.85);

    let toon_output = encode_to_toon_with_fallback(&response, "test");

    // Should produce output
    assert!(!toon_output.is_empty());
    assert!(toon_output.contains("getUserData"));
}

#[test]
fn test_toon_round_trip() {
    let symbols = vec![create_test_symbol("testFunc", 5)];
    let original = OptimizedResponse::new("fast_search", symbols, 0.90);

    // Encode to TOON
    let toon_str = toon_format::encode_default(&original)
        .expect("TOON encoding should succeed");

    // Decode back
    let decoded: OptimizedResponse<Symbol> = toon_format::decode_default(&toon_str)
        .expect("TOON decoding should succeed");

    // Verify lossless round-trip
    assert_eq!(original.tool, decoded.tool);
    assert_eq!(original.results.len(), decoded.results.len());
    assert_eq!(original.confidence, decoded.confidence);
    assert_eq!(original.results[0].name, decoded.results[0].name);
}

#[test]
fn test_toon_token_savings() {
    // Create 10 symbols to measure savings
    let symbols: Vec<Symbol> = (0..10)
        .map(|i| create_test_symbol(&format!("func_{}", i), i * 10))
        .collect();

    let response = OptimizedResponse::new("fast_search", symbols, 0.90);

    // JSON encoding
    let json_output = serde_json::to_string_pretty(&response)
        .expect("JSON encoding should succeed");

    // TOON encoding
    let toon_output = toon_format::encode_default(&response)
        .expect("TOON encoding should succeed");

    let json_len = json_output.len();
    let toon_len = toon_output.len();
    let savings_pct = ((json_len - toon_len) as f64 / json_len as f64) * 100.0;

    println!("JSON: {} chars", json_len);
    println!("TOON: {} chars", toon_len);
    println!("Savings: {:.1}%", savings_pct);

    // TOON should be smaller
    assert!(toon_len < json_len, "TOON should be smaller than JSON");
    // Should achieve at least 20% savings
    assert!(
        savings_pct > 20.0,
        "Should achieve at least 20% token savings (got {:.1}%)",
        savings_pct
    );
}

#[test]
fn test_fallback_function_exists() {
    // Verify the fallback function works with empty data
    let response: OptimizedResponse<Symbol> = OptimizedResponse::new("fast_search", vec![], 0.0);
    let output = encode_to_toon_with_fallback(&response, "empty test");

    assert!(!output.is_empty());
    assert!(output.contains("fast_search"));
}

#[test]
fn test_toon_with_insights_and_actions() {
    let symbols = vec![create_test_symbol("testFunc", 10)];
    let response = OptimizedResponse::new("fast_search", symbols, 0.85)
        .with_insights("Test insights".to_string())
        .with_next_actions(vec!["Action 1".to_string()]);  // Single action to avoid array issues

    // Verify encoding works
    let toon_output = toon_format::encode_default(&response)
        .expect("TOON should encode with insights and actions");

    // Verify content is in the output
    assert!(toon_output.contains("Test insights"));
    assert!(toon_output.contains("Action 1"));

    // Note: Full round-trip decode with arrays has known issues in TOON v0.3.6
    // Our fallback to JSON ensures this works in production
}
