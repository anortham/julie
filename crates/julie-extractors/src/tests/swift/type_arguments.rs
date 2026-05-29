//! Swift ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Swift generics use angle-bracket syntax: `Array<Int>`, `Dictionary<String,User>`.
//! Generic type uses appear in property annotations, parameter types, and
//! return types. `user_type` is the grammar node that carries both a
//! `type_identifier` (the base name) and a `type_arguments` list.
//!
//! Nested generics are captured as `children` of the enclosing usage (one
//! `TypeArgumentUsage` per outermost generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::swift::SwiftExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .expect("load Swift grammar");
    let tree = parser.parse(code, None).expect("parse Swift");
    let mut ext = SwiftExtractor::new(
        "swift".to_string(),
        "test.swift".to_string(),
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
fn property_generic_type_records_single_argument() {
    // `let items: Array<User>` — Array is outermost; User is ordinal 0.
    let code = r#"
class User {}
class Repo {
    let items: Array<User>
}
"#;
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
fn nested_generic_preserves_order_and_nesting() {
    // `Array<Dictionary<String, Int>>` — Array is outermost; Dictionary is nested.
    // Top-level args: [(0, "Dictionary")]. Dictionary carries children (0,"String"),(1,"Int").
    // `Dictionary` inside Array's args must NOT produce a second TypeArgumentUsage row.
    let code = r#"
class Repo {
    let mapping: Array<Dictionary<String, Int>>
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Array is the only outermost generic use site (Dictionary<...> is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "Dictionary")]);
    assert_eq!(
        args[0]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "String"), (1, "Int")],
        "Dictionary<String,Int> nested children preserved under ordinal 0"
    );
}

#[test]
fn two_arg_generic_records_ordered_pair() {
    // `Dictionary<String, User>` — two top-level args in declared order.
    let code = r#"
class User {}
class Store {
    let cache: Dictionary<String, User>
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "one generic use site (Dictionary<String,User>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "String"), (1, "User")],
        "key/value order must be preserved"
    );
}

#[test]
fn construction_generic_records_argument() {
    // `Box<User>()` — Swift constructor call; Box is outermost, User is ordinal 0.
    // Parsed as constructor_expression { constructed_type: user_type { type_identifier("Box"), type_arguments } }.
    let code = r#"
class User {}
class Box<T> {
    init() {}
}
func test() {
    let _ = Box<User>()
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Box<User>()), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn heritage_generic_records_argument() {
    // `class D: Base<User>` — Swift inheritance clause; Base is outermost, User is ordinal 0.
    // Parsed via inheritance_specifier { inherits_from: user_type { type_identifier("Base"), type_arguments } }.
    let code = r#"
class User {}
class Base<T> {}
class D: Base<User> {}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Base<User> in heritage), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn non_generic_type_records_no_arguments() {
    // `let x: Container` with no `<...>` is not a generic application — zero rows.
    let code = r#"
class Container {}
class Repo {
    let x: Container
}
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}
