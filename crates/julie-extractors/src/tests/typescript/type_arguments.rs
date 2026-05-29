//! TypeScript ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Every generic *use site* (`extends Base<Foo,Bar>`, `new Map<string,User>()`) must emit
//! its applied type arguments in order, with nesting preserved, attached to the use-site
//! identifier. Order is the whole point — assert exact ordinals, not set membership. Nested
//! generics are captured as children of their enclosing usage (one usage per outermost
//! generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::typescript::TypeScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .expect("load TypeScript grammar");
    let tree = parser.parse(code, None).expect("parse TypeScript");
    let mut ext = TypeScriptExtractor::new(
        "typescript".to_string(),
        "test.ts".to_string(),
        code.to_string(),
        &PathBuf::from("/test/workspace"),
    );
    let symbols = ext.extract_symbols(&tree);
    ext.extract_identifiers(&tree, &symbols);
    ext.base().type_argument_usages.clone()
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
fn heritage_clause_generic_records_ordered_pair() {
    // `Base<Foo, Bar>` in the extends clause is a single outermost generic use site.
    // Foo and Bar ride along as ordered arguments; no separate usages for them.
    let code = "class A extends Base<Foo, Bar> {}";
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Base<Foo,Bar>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Foo"), (1, "Bar")],
        "Base<Foo,Bar> source-vs-dest order must be preserved"
    );
    assert!(usages[0].arguments[0].children.is_empty());
    assert!(usages[0].arguments[1].children.is_empty());
}

#[test]
fn new_expression_generic_records_ordered_pair() {
    // `new Map<string, User>()` — single outermost generic on the constructor call identifier.
    let code = r#"
function f() {
    const m = new Map<string, User>();
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Map<string,User>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "string"), (1, "User")],
        "Map<string,User> type-arg order must be preserved"
    );
    assert!(usages[0].arguments[0].children.is_empty());
    assert!(usages[0].arguments[1].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `new Map<string, Array<User>>()` — Map is the outermost; Array<User> is nested.
    // The Array usage must NOT be double-counted as a separate top-level usage.
    let code = r#"
function f() {
    const m = new Map<string, Array<User>>();
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost generic use site (Array<User> is nested), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "string"), (1, "Array")],
        "outer arguments: string at 0, Array at 1"
    );
    assert!(
        usages[0].arguments[0].children.is_empty(),
        "string has no nested args"
    );
    assert_eq!(
        usages[0].arguments[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "User")],
        "Array<User> nested argument preserved under ordinal 1"
    );
}

#[test]
fn non_generic_new_expression_records_no_arguments() {
    // `new Map()` with no type arguments must produce zero type_argument_usages rows.
    let code = r#"
function f() {
    const m = new Map();
}
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic new expression must record no type arguments, got {usages:?}"
    );
}
