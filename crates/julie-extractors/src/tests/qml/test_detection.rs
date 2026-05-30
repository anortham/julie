//! QML test-role detection support (Miller bridge test detection).
//!
//! Qt Quick Test files declare tests as a `TestCase { ... }` root component. There
//! are no annotations, so the post-extraction classifier (src/analysis/test_roles.rs)
//! recognizes the container by its base type: the qml extractor must emit
//! `base_types = ["TestCase"]` on the root component symbol, and `qml.toml`'s
//! `test_base_types = ["TestCase"]` then flags it as a `TestContainer`. These tests
//! assert the extractor half of that contract (the metadata emission); the
//! classifier half is covered by src/tests/analysis/test_roles_tests.rs.

use super::extract_symbols;
use crate::base::SymbolKind;

/// Pull the `base_types` metadata array (strings) off a symbol, if present.
fn base_types(symbol: &crate::base::Symbol) -> Vec<String> {
    symbol
        .metadata
        .as_ref()
        .and_then(|m| m.get("base_types"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn testcase_root_component_emits_base_types_metadata() {
    // A `TestCase { ... }` root is the Qt Quick Test container. The root component
    // is extracted as a Class whose `base_types` records the component type so the
    // classifier can match it against `test_base_types = ["TestCase"]`.
    let code = r#"
import QtTest 1.0

TestCase {
    name: "MathTests"

    function test_addition() {
        compare(1 + 1, 2);
    }
}
"#;
    let symbols = extract_symbols(code);
    let root = symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Class)
        .unwrap_or_else(|| panic!("expected a root component Class, got {symbols:?}"));
    assert_eq!(
        base_types(root),
        vec!["TestCase".to_string()],
        "root component must record its base type under `base_types` for the test-role classifier"
    );
}

#[test]
fn non_test_root_component_records_its_own_base_type() {
    // The `base_types` mechanism is general (config-driven, not a TestCase special
    // case): a plain `Rectangle { }` root records `["Rectangle"]`, which simply does
    // not match `test_base_types`, so it is NOT a test container.
    let code = r#"
import QtQuick 2.0

Rectangle {
    width: 100
    height: 100
}
"#;
    let symbols = extract_symbols(code);
    let root = symbols
        .iter()
        .find(|s| s.kind == SymbolKind::Class)
        .unwrap_or_else(|| panic!("expected a root component Class, got {symbols:?}"));
    assert_eq!(base_types(root), vec!["Rectangle".to_string()]);
}
