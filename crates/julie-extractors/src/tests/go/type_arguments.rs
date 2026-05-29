//! Go ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Go 1.18+ generics use bracket syntax: `Container[int]`, `Map[string, List[int]]`.
//! Generic types appear in variable declarations, composite literals, and function
//! calls with explicit type parameters (`GetValue[string](c)`).
//!
//! Nested generics are captured as `children` of the enclosing usage (one
//! `TypeArgumentUsage` per outermost generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::go::GoExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .expect("load Go grammar");
    let tree = parser.parse(code, None).expect("parse Go");
    let mut ext = GoExtractor::new(
        "go".to_string(),
        "test.go".to_string(),
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
fn var_decl_generic_type_records_single_argument() {
    // `var x Container[int]` — Container is the outermost generic; int is ordinal 0.
    let code = r#"package main

type Container[T any] struct{ Value T }

var x Container[int]
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Container[int]), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "int")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `var x Map[string, List[int]]` — Map is outermost; List[int] is nested.
    // Top-level args: (0, "string"), (1, "List"). List carries child (0, "int").
    // `List` inside Map's args must NOT produce a second TypeArgumentUsage row.
    let code = r#"package main

type List[T any] struct{}
type Map[K, V any] struct{}

var x Map[string, List[int]]
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost generic use site (List[int] is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "string"), (1, "List")]);
    assert!(args[0].children.is_empty(), "string has no nested args");
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "int")],
        "List[int] nested argument preserved under ordinal 1"
    );
}

#[test]
fn generic_function_call_records_type_argument() {
    // `GetValue[string](c)` — generic function call with explicit type arg.
    let code = r#"package main

func GetValue[T any](c interface{}) T {
    var r T
    return r
}

func main() {
    _ = GetValue[string](nil)
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "one generic function call (GetValue[string]), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "string")],
        "GetValue[string] type argument must be captured in order"
    );
}

#[test]
fn composite_literal_construction_records_type_argument() {
    // `Container[int]{}` — composite literal construction; Container is outermost, int is ordinal 0.
    // Grammar: composite_literal { type: generic_type { type: type_identifier("Container") } }.
    // The generic_type parent of type_identifier is composite_literal (not type_elem) → outermost.
    let code = r#"package main

type Container[T any] struct{ Value T }

func main() {
    _ = Container[int]{}
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Container[int]{{}}), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "int")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn non_generic_type_records_no_arguments() {
    // `var x Container` with no `[...]` is not a generic application — zero rows.
    let code = r#"package main

type Container struct{}

var x Container
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}
