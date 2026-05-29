//! GDScript ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! GDScript uses bracket syntax for generic types: `Array[String]`,
//! `Dictionary[String, int]`. The grammar node is `subscript` (a child of
//! `type`), with `subscript_arguments` holding the comma-separated type args
//! as identifier or nested subscript children.
//!
//! Nested generics (`Array[Array[int]]`) ride along as `children` of the
//! outermost usage — one TypeArgumentUsage row per outermost generic.

use crate::base::TypeArgumentUsage;
use crate::gdscript::GDScriptExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_gdscript::LANGUAGE.into())
        .expect("load GDScript grammar");
    let tree = parser.parse(code, None).expect("parse GDScript");
    let mut ext = GDScriptExtractor::new(
        "gdscript".to_string(),
        "test.gd".to_string(),
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
fn var_single_generic_records_one_argument() {
    // `var items: Array[User]` — single outermost generic, User at ordinal 0.
    let code = r#"
class_name Repo
var items: Array[User]
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Array[User]), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn dict_two_args_records_ordered_pair() {
    // `var counts: Dictionary[String, int]` — two ordered type arguments.
    let code = r#"
class_name Repo
var counts: Dictionary[String, int]
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Dictionary[String,int]), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "String"), (1, "int")],
        "Dictionary[String,int] ordinal order must be preserved"
    );
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `var nested: Array[Array[int]]` — Array is outermost; inner Array[int] is nested.
    // Only one TypeArgumentUsage row (outermost Array); inner rides as a child.
    let code = r#"
class_name Repo
var nested: Array[Array[int]]
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Array is the only outermost use site (inner Array[int] is nested), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Array")],
        "outer argument: inner Array at ordinal 0"
    );
    assert_eq!(
        usages[0].arguments[0]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "int")],
        "inner Array[int] child preserved under ordinal 0"
    );
}

#[test]
fn construction_new_call_records_no_arguments() {
    // `MyClass.new()` — GDScript construction idiom. No generic type arguments.
    // Grammar: call { attribute { ... } } — there is no generic construction node;
    // `subscript` only appears inside a `type` annotation context. CONSTRUCTION → N/A.
    let code = r#"
class_name Repo
func test():
    var inst = MyClass.new()
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "GDScript .new() construction carries no type arguments, got {usages:?}"
    );
}

#[test]
fn non_generic_type_records_no_arguments() {
    // `var x: Container` with no `[...]` — zero rows.
    let code = r#"
class_name Repo
var items: Container
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}
