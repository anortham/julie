//! Kotlin ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Every generic *use site* (`List<User>` field, `Map<String, Int>`,
//! `mutableListOf<User>()`, `extends Base<T>`, nesting) must emit its applied
//! type arguments in order, with nesting preserved. Order is the whole point —
//! assert exact ordinals. Nested generics ride along as `children` of the
//! enclosing usage (one usage per outermost generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::kotlin::KotlinExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
        .expect("load Kotlin grammar");
    let tree = parser.parse(code, None).expect("parse Kotlin");
    let mut ext = KotlinExtractor::new(
        "kotlin".to_string(),
        "test.kt".to_string(),
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
fn property_single_generic_records_one_argument() {
    // `List<User>` property — single outermost generic use site.
    let code = r#"
class Repo {
    val items: List<User> = emptyList()
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (List<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn map_two_args_records_ordered_pair() {
    // `Map<String, Int>` — two ordered type arguments.
    // Kotlin noise-filter applies to identifiers, not type-arg values; String
    // and Int must still appear in the type_arguments capture.
    let code = r#"
class Repo {
    val counts: Map<String, Int> = emptyMap()
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Map<String,Int>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "String"), (1, "Int")],
        "Map<String,Int> ordinal order must be preserved"
    );
}

#[test]
fn call_expression_generic_records_argument() {
    // `mutableListOf<User>()` — generic on the call expression.
    let code = r#"
class Repo {
    fun init() {
        val items = mutableListOf<User>()
    }
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (mutableListOf<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn supertype_generic_records_type_arg() {
    // `class Box : Base<T>` — generic in supertype clause.
    let code = r#"
class Box : Base<Item>()
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "one generic use site in supertype clause (Base<Item>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "Item")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `List<Map<String, Int>>` — List is the outermost; Map<String,Int> is nested.
    // The Map usage must NOT be double-counted as a separate top-level usage.
    let code = r#"
class Repo {
    val index: List<Map<String, Int>> = emptyList()
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "List is the only outermost generic use site (Map<String,Int> is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Map")],
        "outer argument: Map at ordinal 0"
    );
    assert_eq!(
        args[0]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "String"), (1, "Int")],
        "Map<String,Int> nested arguments preserved under ordinal 0"
    );
}

#[test]
fn non_generic_type_records_no_arguments() {
    // Plain `List` with no `<...>` — zero rows.
    let code = r#"
class Repo {
    val items: Collection = emptyList()
}
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}
