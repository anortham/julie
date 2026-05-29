//! C++ ordered/nested generic type-argument capture (Miller bridge Phase 2).
//!
//! C++ generics use angle-bracket syntax on template specializations:
//! `Box<Item>`, `Map<Key, Value>`, `Map<int, Vec<Item>>`.
//!
//! Grammar nodes: `template_type { name: type_identifier, arguments: template_argument_list }`.
//! `template_argument_list` children are `type_descriptor` wrappers around each type arg.
//!
//! Nested generics are captured as `children` of the enclosing usage (one
//! `TypeArgumentUsage` per outermost generic), never double-counted.

use crate::base::TypeArgumentUsage;
use crate::tests::cpp::parse_cpp;

fn capture(code: &str) -> Vec<TypeArgumentUsage> {
    let (mut ext, tree) = parse_cpp(code);
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
    // `Box<Item>` — Box is outermost; Item is ordinal 0.
    let code = r#"
template<typename T> struct Box {};
struct Item {};
Box<Item> b;
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Box<Item>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "Item")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn nested_generic_preserves_order_and_nesting() {
    // `Map<int, Vec<Item>>` — Map is outermost; Vec<Item> is nested.
    // Top-level: (0,"int"), (1,"Vec"). Vec carries child (0,"Item").
    // `Vec` inside Map's args must NOT produce a second TypeArgumentUsage row.
    let code = r#"
template<typename K, typename V> struct Map {};
template<typename T> struct Vec {};
struct Item {};
Map<int, Vec<Item>> m;
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "Map is the only outermost generic (Vec<Item> is nested), got {usages:?}"
    );
    let args = &usages[0].arguments;
    assert_eq!(top_level(&usages[0]), vec![(0, "int"), (1, "Vec")]);
    assert!(args[0].children.is_empty(), "int has no nested args");
    assert_eq!(
        args[1]
            .children
            .iter()
            .map(|c| (c.ordinal, c.type_name.as_str()))
            .collect::<Vec<_>>(),
        vec![(0, "Item")],
        "Vec<Item> nested argument preserved under ordinal 1"
    );
}

#[test]
fn non_type_arg_preserves_ordinal() {
    // `Arr<Item, 5>` — Arr has a type arg (Item, ordinal 0) AND a non-type arg (5, ordinal 1).
    // Dropping non-type args would shift ordinals; both must be recorded.
    let code = r#"
template<typename T, int N> struct Arr {};
struct Item {};
Arr<Item, 5> a;
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Arr<Item, 5>), got {usages:?}"
    );
    assert_eq!(
        top_level(&usages[0]),
        vec![(0, "Item"), (1, "5")],
        "non-type arg `5` must appear at ordinal 1, not be dropped"
    );
}

#[test]
fn template_call_records_type_argument() {
    // `make_shared<Foo>()` — template function call; Foo is the type arg at ordinal 0.
    let code = r#"
struct Foo {};
template<typename T> T* make_shared() { return nullptr; }
void test() { make_shared<Foo>(); }
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (make_shared<Foo>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "Foo")]);
}

#[test]
fn template_base_class_records_type_argument() {
    // `struct D : Base<Item> {}` — template base class.
    // Base is outermost (inside base_specifier, NOT type_descriptor), Item is ordinal 0.
    let code = r#"
struct Item {};
template<typename T> struct Base {};
struct D : Base<Item> {};
"#;
    let usages = capture(code);
    // D itself produces no TypeArgumentUsage (it's a declaration, not a use site).
    // Base<Item> is a use site — Base records [(0,"Item")].
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Base<Item>), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "Item")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn object_construction_generic_records_argument() {
    // `Box<Item>{}` — aggregate-initializer construction; Box is outermost, Item is ordinal 0.
    // Parsed as compound_literal_expression { type: template_type { name: Box, ... } }.
    let code = r#"
template<typename T> struct Box {};
struct Item {};
void test() { Box<Item>{}; }
"#;
    let usages = capture(code);
    assert_eq!(
        usages.len(),
        1,
        "exactly one generic use site (Box<Item>{{}}), got {usages:?}"
    );
    assert_eq!(top_level(&usages[0]), vec![(0, "Item")]);
    assert!(usages[0].arguments[0].children.is_empty());
}

#[test]
fn non_generic_type_records_no_arguments() {
    // Plain `Item x` with no `<...>` — not a generic use site.
    let code = r#"
struct Item {};
Item x;
"#;
    let usages = capture(code);
    assert!(
        usages.is_empty(),
        "non-generic type must record no type arguments, got {usages:?}"
    );
}
