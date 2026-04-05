//! Tests for the edit_symbol tool's pure editing functions.
//!
//! These test replace_symbol_body and insert_near_symbol directly,
//! not the full MCP tool flow (which requires an indexed workspace).

use crate::tools::editing::edit_symbol::{insert_near_symbol, replace_symbol_body};

#[test]
fn test_replace_symbol_body() {
    let source = "fn hello() {\n    println!(\"hello\");\n}\n\nfn world() {\n    println!(\"world\");\n}\n";

    let result = replace_symbol_body(source, 1, 3, "fn hello() {\n    println!(\"goodbye\");\n}")
        .expect("Replace should succeed");

    assert!(result.contains("goodbye"), "Should contain new body");
    assert!(result.contains("fn world()"), "Should preserve other functions");
    assert!(
        !result.contains("println!(\"hello\")"),
        "Should not contain old body"
    );
}

#[test]
fn test_insert_after_symbol() {
    let source = "struct Foo {\n    x: i32,\n}\n\nfn bar() {}\n";

    let result = insert_near_symbol(
        source,
        3,
        "\nimpl Foo {\n    fn new() -> Self { Self { x: 0 } }\n}",
        "after",
    )
    .expect("Insert after should succeed");

    assert!(result.contains("impl Foo"), "Should contain inserted code");
    let struct_pos = result.find("struct Foo").unwrap();
    let impl_pos = result.find("impl Foo").unwrap();
    let bar_pos = result.find("fn bar").unwrap();
    assert!(struct_pos < impl_pos, "impl should be after struct");
    assert!(impl_pos < bar_pos, "impl should be before bar");
}

#[test]
fn test_insert_before_symbol() {
    let source = "fn process() {\n    // work\n}\n";

    let result = insert_near_symbol(source, 1, "/// Process all items.", "before")
        .expect("Insert before should succeed");

    let doc_pos = result.find("/// Process all items.").unwrap();
    let fn_pos = result.find("fn process()").unwrap();
    assert!(doc_pos < fn_pos, "Doc comment should be before function");
}

#[test]
fn test_replace_preserves_surrounding_content() {
    let source =
        "// header comment\n\nfn target() {\n    old_code();\n}\n\n// footer comment\n";

    let result = replace_symbol_body(source, 3, 5, "fn target() {\n    new_code();\n}")
        .expect("Replace should succeed");

    assert!(result.contains("// header comment"), "Should preserve header");
    assert!(result.contains("// footer comment"), "Should preserve footer");
    assert!(result.contains("new_code()"), "Should contain new code");
}

#[test]
fn test_invalid_line_range() {
    let source = "fn hello() {}\n";
    let result = replace_symbol_body(source, 5, 10, "new code");
    assert!(result.is_err(), "Should fail for out-of-range lines");
}

#[test]
fn test_insert_at_invalid_line() {
    let source = "fn hello() {}\n";
    let result = insert_near_symbol(source, 100, "new code", "after");
    assert!(result.is_err(), "Should fail for out-of-range line");
}

#[test]
fn test_replace_helper_is_unguarded() {
    // replace_symbol_body is a pure line-manipulation helper with no freshness check.
    // The freshness guard lives in EditSymbolTool::call_tool (blake3 hash comparison).
    // This test documents that the helper applies blindly -- callers must verify freshness.
    let modified_file = "line1\nnew_line_inserted\nfn foo() {\n    bar()\n}\nline5\n";
    let result = replace_symbol_body(modified_file, 2, 4, "fn foo() {\n    baz()\n}");
    assert!(result.is_ok());
    let content = result.unwrap();
    // The helper replaces lines 2-4 regardless of what's there.
    // In a stale-index scenario, this produces wrong output.
    // call_tool's freshness check prevents this from happening in practice.
    assert!(!content.contains("fn foo() {\n    bar()"), "Old foo body should be replaced");
}
