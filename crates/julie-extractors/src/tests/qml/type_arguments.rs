//! QML ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! QML-JS shares the TypeScript grammar, so generic types use the same
//! angle-bracket syntax: `Array<User>`, `Map<string, User>`.
//! The grammar node is `generic_type` with a `name` field (`type_identifier`)
//! and a `type_arguments` field (`type_arguments` node whose unnamed children
//! are concrete type nodes: `type_identifier`, `generic_type`, `predefined_type`).
//!
//! Nested generics ride along as `children` of the outermost usage
//! (`Map<string, Array<User>>` → one row for Map; Array is a child).

use crate::base::TypeArgumentUsage;
use crate::qml::QmlExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_qmljs::LANGUAGE.into())
        .expect("load QML grammar");
    let tree = parser.parse(code, None).expect("parse QML");
    let mut ext = QmlExtractor::new(
        "qml".to_string(),
        "test.qml".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.get_type_argument_usages()
}

/// Flatten a usage's top-level arguments to `(ordinal, type_name)` pairs.
fn top_level(usage: &TypeArgumentUsage) -> Vec<(u32, &str)> {
    usage
        .arguments
        .iter()
        .map(|arg| (arg.ordinal, arg.type_name.as_str()))
        .collect()
}

#[test]
fn function_param_single_generic_records_one_argument() {
    // `Array<User>` parameter type — single outermost generic, User at ordinal 0.
    let code = r#"import QtQuick 2.0
Item {
    function process(items: Array<User>): void {}
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Array<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn two_arg_generic_records_ordered_pair() {
    // `Map<Key, Value>` — two ordered type arguments.
    let code = r#"import QtQuick 2.0
Item {
    function process(m: Map<Key, Value>): void {}
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Map<Key,Value>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Key"), (1, "Value")],
        "Map<Key,Value> ordinal order must be preserved"
    );
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `Map<Key, Array<User>>` — Map is outermost; Array<User> is nested.
    // Only one TypeArgumentUsage row (for Map); Array rides as a child.
    let code = r#"import QtQuick 2.0
Item {
    function process(m: Map<Key, Array<User>>): void {}
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost use site (Array<User> is nested), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Key"), (1, "Array")],
        "outer arguments: Key at 0, Array at 1"
    );
    assert_eq!(
        usages[0].arguments[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "User")],
        "Array<User> nested child preserved under ordinal 1"
    );
}

#[test]
fn non_generic_type_records_no_arguments() {
    // Plain `Container` with no `<...>` — zero rows.
    let code = r#"import QtQuick 2.0
Item {
    function process(item: Container): void {}
}"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}

#[test]
fn construction_new_expression_records_type_args() {
    // `new Map<string, User>()` — construction with type arguments.
    // QML-JS grammar: `new_expression { constructor, type_arguments, arguments }`.
    // The `type_arguments` field is a DIRECT sibling of `constructor` on
    // `new_expression`, NOT wrapped in `generic_type`.
    let code = r#"import QtQuick 2.0
Item {
    function test() {
        var m = new Map<string, User>();
    }
}"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic construction (new Map<string,User>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "string"), (1, "User")],
        "construction type args must be ordered (0,string) (1,User)"
    );
}

#[test]
fn heritage_extends_clause_grammar_na() {
    // `class Repo extends Container<User>` — VERIFIED-N/A (independently verified).
    //
    // PARSE TREE EVIDENCE (diagnostic dump of `class C extends Base<string> { }`):
    //   program
    //     ui_object_definition        ← tree-sitter-qmljs treats `class` as a QML
    //       identifier "class"           component type identifier (like Rectangle),
    //       ERROR "C extends Base<string>" ← NOT a JS class_declaration
    //       ui_object_initializer "{ }"
    //
    // The grammar does NOT produce an `extends_clause` or `class_heritage` node at
    // all — `class C extends Base<T>` is parsed as a QML `ui_object_definition`
    // where "C extends Base<string>" becomes an ERROR node. There are ZERO
    // `type_arguments` nodes anywhere in the parse tree.
    //
    // Root cause: tree-sitter-qmljs is a QML-first parser; `class` is a QML keyword
    // for UI component objects (not JS class declarations). JS-style `class C extends
    // B<T>` is not recognized as a class declaration — the heritage subtree never
    // materializes. No `extends_clause` path to implement.
    //
    // QML construction (`new Map<string, User>()`) IS captured — only JS-style
    // heritage generics are N/A. This test locks in the 0-row behavior so any
    // future grammar change will require an intentional update.
    let code = r#"class Repo extends Container<User> {
}"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "QML heritage generic extends does not produce type_arguments (grammar N/A), got {usages:?}"
    );
}
