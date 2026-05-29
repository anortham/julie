//! Dart ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Dart generics use angle-bracket syntax: `List<User>`, `Map<String, int>`.
//! The grammar node `type_arguments` contains `type` children directly (no
//! wrapper like Kotlin's `type_projection`). The `type` node holds a
//! `type_identifier` (base name) and an optional nested `type_arguments`.
//!
//! Nested generics ride along as `children` of the outermost usage
//! (`List<Map<String,int>>` → one row for List; Map is a child, not a row).

use crate::base::TypeArgumentUsage;
use crate::dart::DartExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_dart::LANGUAGE.into())
        .expect("load Dart grammar");
    let tree = parser.parse(code, None).expect("parse Dart");
    let mut ext = DartExtractor::new(
        "dart".to_string(),
        "test.dart".to_string(),
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
fn field_single_generic_records_one_argument() {
    // `List<User>` field — single outermost generic, User at ordinal 0.
    let code = r#"
class Repo {
  List<User> items;
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
    // `Map<String, int>` — two ordered type arguments.
    let code = r#"
class Repo {
  Map<String, int> counts;
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Map<String,int>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "String"), (1, "int")],
        "Map<String,int> ordinal order must be preserved"
    );
}

#[test]
fn future_generic_records_single_argument() {
    // `Future<User>` return type on a function.
    let code = r#"
class Service {
  Future<User> fetchUser() {
    return Future.value(null);
  }
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Future<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `List<Map<String, int>>` — List is outermost; Map<String,int> is nested.
    // Only one TypeArgumentUsage row (for List); Map rides as a child.
    let code = r#"
class Repo {
  List<Map<String, int>> index;
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "List is the only outermost use site (Map<String,int> is nested), got {usages:?}"
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
        vec![(0, "String"), (1, "int")],
        "Map<String,int> nested children preserved under ordinal 0"
    );
}

#[test]
fn non_generic_type_records_no_arguments() {
    // Plain `Container` with no `<...>` — zero rows.
    let code = r#"
class Repo {
  Container items;
}
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic use must record no type arguments, got {usages:?}"
    );
}

#[test]
fn construction_new_generic_records_argument() {
    // `new List<User>()` — construction use site; should capture the type argument.
    let code = r#"
class Repo {
  Repo() {
    var items = new List<User>();
  }
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (new List<User>()), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn heritage_extends_generic_records_argument() {
    // `class MyList extends List<User>` — heritage use site.
    let code = r#"
class MyList extends List<User> {}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (extends List<User>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

