//! Rust ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Every generic *use site* (`items: Vec<String>`, `HashMap<K, V>`,
//! `foo::<T>()` turbofish) must emit its applied type arguments in order,
//! with nesting preserved, attached to the use-site identifier.  Order is the
//! whole point — assert exact ordinals, not set membership.  Nested generics
//! are captured as children of their enclosing usage (one usage per outermost
//! generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::rust::RustExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("load Rust grammar");
    let tree = parser.parse(code, None).expect("parse Rust");
    let mut ext = RustExtractor::new(
        "rust".to_string(),
        "test.rs".to_string(),
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
fn field_generic_type_records_single_ordered_argument() {
    // `Vec<String>` in a struct field is a single-arg generic use site.
    let code = "struct Repo { items: Vec<String>, }";
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Vec<String>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "String")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `HashMap<String, Vec<u8>>` — HashMap is outermost; Vec<u8> is nested.
    // Top-level args: (0, "String"), (1, "Vec"). Vec carries child (0, "u8").
    // `Vec` inside HashMap's args must NOT create a second TypeArgumentUsage row.
    let code = r#"
fn f() {
    let _: HashMap<String, Vec<u8>>;
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "HashMap is the only outermost generic use site (Vec<u8> is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "String"), (1, "Vec")]);
    assert!(args[0].children.is_empty(), "String has no nested args");
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "u8")],
        "Vec<u8> nested argument preserved under ordinal 1"
    );
}

#[test]
fn turbofish_call_records_ordered_argument() {
    // `foo::<String>()` — turbofish type argument on a generic function call.
    // The `generic_function` node carries the type_arguments; the call
    // identifier is "foo".
    let code = r#"
fn foo<T>() {}
fn bar() { foo::<String>(); }
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "one turbofish generic call (foo::<String>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "String")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn struct_literal_turbofish_records_type_argument() {
    // `Repo::<String> { value: ... }` — explicit turbofish on struct literal.
    // Grammar: struct_expression { name: generic_type_with_turbofish { type: type_identifier("Repo"),
    //   type_arguments: <String> } }.
    // String is ordinal 0.
    let code = r#"
struct Repo<T> { value: T }
fn build() {
    let _ = Repo::<String> { value: String::new() };
}
"#;
    let usages = capture(code);
    let with_string: Vec<_> = usages
        .iter()
        .filter(|u| u.arguments.iter().any(|a| a.type_name == "String"))
        .collect();
    assert_eq!(
        with_string.len(),
        1,
        "Repo::<String> struct literal must be captured, got {usages:?}"
    );
    assert_eq!(top_level(with_string[0]), vec![(0, "String")]);
}

#[test]
fn impl_heritage_records_trait_type_argument() {
    // `impl From<String> for Repo` — heritage clause `From<String>` is a
    // generic type use site. Grammar: impl_item { trait: generic_type {
    //   type: type_identifier("From"), type_arguments { type_identifier("String") }
    // } }. The `From` type_identifier is captured as TypeUsage with String at ordinal 0.
    // NOTE: Rust has no class inheritance; the impl trait clause is the heritage analogue.
    let code = r#"
struct Repo;
impl From<String> for Repo {
    fn from(_: String) -> Self { Repo }
}
"#;
    let usages = capture(code);
    // There will be usages from the `String` parameter type and possibly others.
    // The one we care about: the usage where the identifier is "From" with [(0,"String")].
    let from_usage = usages
        .iter()
        .find(|u| u.arguments.len() == 1 && u.arguments[0].type_name == "String");
    assert!(
        from_usage.is_some(),
        "impl From<String> must record String as a type argument, got {usages:?}"
    );
    assert_eq!(top_level(from_usage.unwrap()), vec![(0, "String")]);
}

#[test]
fn non_generic_type_records_no_arguments() {
    // Plain `String` field with no `<...>` is not a generic application — zero rows.
    let code = "struct Repo { name: String, }";
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic type must record no type arguments, got {usages:?}"
    );
}
