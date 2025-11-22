// Lua helpers inline tests extracted from extractors/lua/helpers.rs

use crate::lua::LuaExtractor;
use crate::lua::helpers::{
    contains_function_definition, find_child_by_type, infer_type_from_expression,
};
use std::path::PathBuf;
use tree_sitter::Parser;

fn init_parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_lua::LANGUAGE.into())
        .expect("Error loading Lua grammar");
    parser
}

/// Helper to find a node by kind anywhere in the tree
fn find_node_by_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_node_by_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

#[test]
fn test_find_child_by_type_returns_none_for_missing_type() {
    let mut parser = init_parser();

    let code = r#"local x = 10"#;
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    // Should return None when child type doesn't exist
    let result = find_child_by_type(root, "nonexistent_type");
    assert!(result.is_none());
}

#[test]
fn test_contains_function_definition_false() {
    let mut parser = init_parser();

    let code = r#"local x = 10"#;
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    // A simple assignment should not contain a function definition
    let has_func = contains_function_definition(root);
    assert!(!has_func, "Should not contain function definition");
}

#[test]
fn test_infer_type_from_expression_string() {
    let mut parser = init_parser();

    let code = r#""hello""#;
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    if let Some(string_node) = find_node_by_kind(root, "string") {
        let workspace_root = PathBuf::from("/tmp/test");
        let base = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let inferred = infer_type_from_expression(base.base(), string_node);
        assert_eq!(
            inferred, "string",
            "String expression should infer as 'string'"
        );
    } else {
        panic!("Could not find string node in parsed code");
    }
}

#[test]
fn test_infer_type_from_expression_number() {
    let mut parser = init_parser();

    let code = r#"42"#;
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    if let Some(number_node) = find_node_by_kind(root, "number") {
        let workspace_root = PathBuf::from("/tmp/test");
        let base = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let inferred = infer_type_from_expression(base.base(), number_node);
        assert_eq!(
            inferred, "number",
            "Number expression should infer as 'number'"
        );
    } else {
        panic!("Could not find number node in parsed code");
    }
}

#[test]
fn test_infer_type_from_expression_table() {
    let mut parser = init_parser();

    let code = r#"{}"#;
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    if let Some(table_node) = find_node_by_kind(root, "table_constructor") {
        let workspace_root = PathBuf::from("/tmp/test");
        let base = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let inferred = infer_type_from_expression(base.base(), table_node);
        assert_eq!(
            inferred, "table",
            "Table constructor should infer as 'table'"
        );
    } else {
        panic!("Could not find table_constructor node in parsed code");
    }
}

#[test]
fn test_infer_type_from_expression_require_call() {
    let mut parser = init_parser();

    let code = r#"require("module")"#;
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    if let Some(call_node) = find_node_by_kind(root, "function_call") {
        let workspace_root = PathBuf::from("/tmp/test");
        let base = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let inferred = infer_type_from_expression(base.base(), call_node);
        assert_eq!(
            inferred, "import",
            "require() call should infer as 'import'"
        );
    } else {
        panic!("Could not find function_call node in parsed code");
    }
}

#[test]
fn test_infer_type_from_expression_unknown() {
    let mut parser = init_parser();

    let code = r#"x"#; // identifier - not directly handled
    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    if let Some(id_node) = find_node_by_kind(root, "identifier") {
        let workspace_root = PathBuf::from("/tmp/test");
        let base = LuaExtractor::new(
            "lua".to_string(),
            "test.lua".to_string(),
            code.to_string(),
            &workspace_root,
        );
        let inferred = infer_type_from_expression(base.base(), id_node);
        assert_eq!(inferred, "unknown", "Identifier should infer as 'unknown'");
    } else {
        panic!("Could not find identifier node in parsed code");
    }
}

// Integration test: verify helpers work in realistic context
#[test]
fn test_helpers_in_extraction_context() {
    let mut parser = init_parser();

    let code = r#"
local x = 10
function test()
    return "hello"
end
"#;

    let tree = parser.parse(code, None).unwrap();
    let root = tree.root_node();

    // Verify that we can traverse the tree using the helpers
    let mut found_something = false;
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        // The helpers should be accessible and usable
        let result = find_child_by_type(child, "identifier");
        if result.is_some() {
            found_something = true;
        }
    }
    assert!(
        found_something,
        "Should have found something in realistic Lua code"
    );
}
