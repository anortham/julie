//! Zig test detection (Miller bridge test-role work).
//!
//! Zig tests are `test_declaration` nodes — structurally unambiguous (the `test`
//! keyword), so the extractor (`zig/functions.rs::extract_test`) emits a Function
//! symbol with `is_test=true` directly, independent of file path or naming. A test
//! is named by a string literal (`test "name" {}`), a bare identifier referencing
//! a decl (`test square {}`), or nothing (`test {}`). All three must be emitted
//! and flagged. There is no `detect_zig` arm in test_detection.rs — detection is
//! structural in the extractor.

use crate::base::{Symbol, SymbolKind};
use crate::zig::ZigExtractor;
use std::path::PathBuf;

fn symbols(file: &str, code: &str) -> Vec<Symbol> {
    let tree = crate::tests::helpers::init_parser(code, "zig");
    let mut ext = ZigExtractor::new(
        "zig".to_string(),
        file.to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    ext.extract_symbols(&tree)
}

fn is_test(sym: &Symbol) -> bool {
    sym.metadata
        .as_ref()
        .and_then(|m| m.get("is_test"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

#[test]
fn string_named_test_is_flagged_path_independent() {
    // In a NON-test path (src/), confirming detection is structural, not path-based.
    let syms = symbols(
        "src/math.zig",
        r#"
test "addition works" {
    try std.testing.expect(2 + 2 == 4);
}
"#,
    );
    let t = syms
        .iter()
        .find(|s| s.name == "addition works")
        .unwrap_or_else(|| panic!("expected a test symbol named after the string, got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(is_test(t), "string-named test must be is_test=true");
}

#[test]
fn identifier_named_test_is_emitted_and_flagged() {
    // `test square {}` — the test is named by a bare identifier, not a string.
    // Before the fix the extractor required a `string` child and emitted nothing.
    let syms = symbols(
        "src/math.zig",
        r#"
test square {
    try std.testing.expect(true);
}
"#,
    );
    let t = syms
        .iter()
        .find(|s| s.name == "square")
        .unwrap_or_else(|| panic!("expected a test symbol named 'square', got {syms:?}"));
    assert_eq!(t.kind, SymbolKind::Function);
    assert!(is_test(t), "identifier-named test must be is_test=true");
}

#[test]
fn anonymous_test_is_emitted_and_flagged() {
    // `test {}` — a fully anonymous test is legal Zig and is still a test.
    let syms = symbols(
        "src/math.zig",
        r#"
test {
    try std.testing.expect(true);
}
"#,
    );
    let t = syms.iter().find(|s| is_test(s)).unwrap_or_else(|| {
        panic!("expected an is_test symbol for the anonymous test, got {syms:?}")
    });
    assert_eq!(t.kind, SymbolKind::Function);
    assert_eq!(
        t.name, "test",
        "anonymous test uses the 'test' placeholder name"
    );
}
