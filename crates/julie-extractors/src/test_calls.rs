//! Shared test call expression extraction for JS/TS test frameworks.
//!
//! Handles Jest, Vitest, Mocha, Bun, and similar test DSLs where tests are
//! call expressions (`it("name", () => {...})`) rather than named function
//! declarations. This module provides constants and extraction logic that can
//! be reused by both the TypeScript and JavaScript extractors.

use crate::base::{BaseExtractor, Symbol, SymbolKind, SymbolOptions};
use std::collections::HashMap;
use tree_sitter::Node;

/// Test block function names — `is_test = true`
pub const TEST_BLOCKS: &[&str] = &["it", "test"];

/// Container block function names — NOT `is_test` (containers for parent tracking)
pub const CONTAINER_BLOCKS: &[&str] = &["describe", "context", "suite"];

/// Lifecycle block function names — `is_test = true`
pub const LIFECYCLE_BLOCKS: &[&str] = &[
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
    "before",
    "after",
];

/// Check if a function name (or its base before `.`) is a test runner call.
///
/// Handles `.skip`, `.only`, `.todo` variants:
/// - `it` -> true
/// - `it.skip` -> true (base "it" matches)
/// - `describe.only` -> true (base "describe" matches)
/// - `randomFn` -> false
pub fn is_test_runner_call(name: &str) -> bool {
    // Split on '.' to handle it.skip, describe.only, test.todo, etc.
    let base = name.split('.').next().unwrap_or(name);
    TEST_BLOCKS.contains(&base)
        || CONTAINER_BLOCKS.contains(&base)
        || LIFECYCLE_BLOCKS.contains(&base)
}

/// Extract a test call expression node into a Symbol.
///
/// The caller must verify that the node is a `call_expression` and the callee
/// name matches `is_test_runner_call` before calling this function.
///
/// # Parameters
/// - `base`: The base extractor for the current file
/// - `node`: A tree-sitter `call_expression` node
/// - `parent_id`: Optional parent symbol ID (e.g., enclosing `describe` block)
///
/// # Returns
/// `Some(Symbol)` if extraction succeeds, `None` if the node structure is unexpected.
pub fn extract_test_call(
    base: &mut BaseExtractor,
    node: Node,
    parent_id: Option<&str>,
) -> Option<Symbol> {
    if node.kind() != "call_expression" {
        return None;
    }

    let function_node = node.child_by_field_name("function")?;

    // Get the full callee name (e.g., "it", "describe", "it.skip")
    let full_callee = base.get_node_text(&function_node);

    // Get the base name for category classification
    let base_name = full_callee.split('.').next().unwrap_or(&full_callee);

    let is_test = TEST_BLOCKS.contains(&base_name);
    let is_container = CONTAINER_BLOCKS.contains(&base_name);
    let is_lifecycle = LIFECYCLE_BLOCKS.contains(&base_name);

    if !is_test && !is_container && !is_lifecycle {
        return None;
    }

    let args_node = node.child_by_field_name("arguments")?;

    // For test/container blocks: extract name from first string argument
    // For lifecycle blocks: use the callee name itself
    let (symbol_name, signature) = if is_lifecycle {
        (base_name.to_string(), format!("{}()", full_callee))
    } else {
        // Find first string argument
        let mut cursor = args_node.walk();
        let first_string = args_node
            .children(&mut cursor)
            .find(|c| c.kind() == "string" || c.kind() == "template_string");

        match first_string {
            Some(string_node) => {
                let raw = base.get_node_text(&string_node);
                let name = raw
                    .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                    .to_string();
                let sig = format!("{}(\"{}\")", full_callee, name);
                (name, sig)
            }
            None => return None, // test/container blocks require a string name
        }
    };

    // Build metadata
    let mut metadata = HashMap::new();
    if is_test || is_lifecycle {
        metadata.insert("is_test".to_string(), serde_json::json!(true));
    }
    if is_container {
        metadata.insert("test_container".to_string(), serde_json::json!(true));
    }
    if is_lifecycle {
        metadata.insert("test_lifecycle".to_string(), serde_json::json!(true));
    }

    let symbol = base.create_symbol(
        &node,
        symbol_name,
        SymbolKind::Function,
        SymbolOptions {
            signature: Some(signature),
            visibility: None,
            parent_id: parent_id.map(|s| s.to_string()),
            metadata: Some(metadata),
            doc_comment: None,
            annotations: Vec::new(),
        },
    );

    Some(symbol)
}
