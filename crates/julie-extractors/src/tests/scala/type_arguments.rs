//! Scala ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! Scala generics use square-bracket syntax: `List[Int]`, `Map[String,User]`.
//! Generic type uses appear in val/var type annotations, method parameter types,
//! return types, and generic method calls (`foo[T](x)`).
//!
//! Nested generics are captured as `children` of the enclosing usage (one
//! `TypeArgumentUsage` per outermost generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::scala::ScalaExtractor;
use std::path::PathBuf;
use tree_sitter::Parser;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_scala::LANGUAGE.into())
        .expect("load Scala grammar");
    let tree = parser.parse(code, None).expect("parse Scala");
    let mut ext = ScalaExtractor::new(
        "scala".to_string(),
        "test.scala".to_string(),
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
    // `val items: List[Item]` — List is outermost; Item is ordinal 0.
    let code = r#"
class Item
class Repo {
  val items: List[Item] = Nil
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (List[Item]), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "Item")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `Map[String, List[Int]]` — Map is outermost; List[Int] is nested.
    // Top-level: (0,"String"), (1,"List"). List carries child (0,"Int").
    // `List` inside Map's args must NOT produce a second TypeArgumentUsage row.
    let code = r#"
class Repo {
  val mapping: Map[String, List[Int]] = Map.empty
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost generic (List[Int] is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "String"), (1, "List")]);
    assert!(args[0].children.is_empty(), "String has no nested args");
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "Int")],
        "List[Int] nested argument preserved under ordinal 1"
    );
}

#[test]
fn generic_function_call_records_argument() {
    // `foo[Item](42)` — generic_function call records type argument.
    let code = r#"
class Item
object Test {
  val r = foo[Item](42)
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "one generic function call (foo[Item]), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "Item")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn function_type_arg_preserves_ordinal() {
    // `Either[String, Int => Boolean]` — two args where the second is a function type.
    // Bug: `_ => None` in decompose_scala_type_arg dropped function_type nodes,
    // causing the arg to vanish entirely. After fix, both ordinals must be present.
    let code = r#"
object Test {
  val result: Either[String, Int => Boolean] = ???
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Either[...]), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]).len(),
        2,
        "both args (String and function type) must be captured; got {:?}",
        top_level(&usages[0])
    );
    let args = top_level(&usages[0]);
    assert_eq!(args[0], (0, "String"), "first arg: String at ordinal 0");
    assert_eq!(
        args[1].0, 1,
        "second arg must be at ordinal 1 (not dropped)"
    );
    // The leaf text for Int => Boolean may include spaces; just check it's non-empty.
    assert!(
        !args[1].1.is_empty(),
        "function-type arg text must be non-empty"
    );
}

#[test]
fn construction_generic_records_argument() {
    // `new Box[User]` — Scala instance creation; Box is outermost, User is ordinal 0.
    // Parsed as instance_expression { generic_type { type_identifier("Box"), type_arguments } }.
    let code = r#"
class User
class Box[T](val value: T)
object Test {
  val b = new Box[User](???)
}
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (new Box[User]), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn heritage_generic_records_argument() {
    // `class Child extends Base[User]` — Scala heritage clause; Base is outermost, User is ordinal 0.
    // Parsed via extends_clause { generic_type { type_identifier("Base"), type_arguments } }.
    let code = r#"
class User
class Base[T]
class Child extends Base[User]
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (extends Base[User]), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "User")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn non_generic_type_records_no_arguments() {
    // Plain `val name: Item` with no `[...]` — not a generic use site.
    let code = r#"
class Item
class Repo {
  val name: Item = ???
}
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic type must record no type arguments, got {usages:?}"
    );
}
